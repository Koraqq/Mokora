//! candle CNN inference. Architecture mirrors `train/train.py` **after** its
//! BatchNorm-folding export, so this side is pure Conv1d + Linear (no BN op).
//!
//! Weights are loaded from a `.safetensors` buffer whose keys are:
//! `conv_in.{weight,bias}`, `res.{i}.conv{1,2}.{weight,bias}`,
//! `fc.{weight,bias}`, `head.{weight,bias}`.

use candle_core::{Device, Result, Tensor};
use candle_nn::{conv1d, linear, Conv1d, Conv1dConfig, Linear, Module, VarBuilder};

use crate::obs::Features;
use crate::Geometry;

/// Backbone width / depth / head of the original built-in weights — the
/// defaults `train/train.py` trains with. A weights file is not bound to
/// them: [`Model::from_safetensors`] reads the size it was trained at off its
/// own tensors, so weights of any width / depth load (and can be compared
/// against each other).
pub const CONV: usize = 64;
pub const BLOCKS: usize = 3;
pub const FC: usize = 256;

/// The backbone size a weights file was trained at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dims {
    pub conv: usize,
    pub blocks: usize,
    pub fc: usize,
}

impl Dims {
    /// Read off the tensors: `conv_in` is `[conv, C, 3]`, `fc` is
    /// `[fc, conv * T]`, and there is one `res.{i}` per block.
    pub fn of(bytes: &[u8]) -> Result<Self> {
        let st = safetensors::SafeTensors::deserialize(bytes)
            .map_err(|e| candle_core::Error::Msg(format!("weights header: {e}")))?;
        let dim0 = |name: &str| -> Result<usize> {
            st.tensor(name)
                .map(|t| t.shape()[0])
                .map_err(|e| candle_core::Error::Msg(format!("weights: {name}: {e}")))
        };
        let blocks = (0..)
            .take_while(|i| st.tensor(&format!("res.{i}.conv1.weight")).is_ok())
            .count();
        Ok(Self {
            conv: dim0("conv_in.weight")?,
            blocks,
            fc: dim0("fc.weight")?,
        })
    }

    /// Numbers stored in a weight file, every tensor counted — the batch
    /// norms are folded into the convolutions at export, so this is a little
    /// under the count the trainer prints.
    pub fn param_count(bytes: &[u8]) -> Result<u64> {
        let st = safetensors::SafeTensors::deserialize(bytes)
            .map_err(|e| candle_core::Error::Msg(format!("weights header: {e}")))?;
        Ok(st
            .tensors()
            .iter()
            .map(|(_, t)| t.shape().iter().product::<usize>() as u64)
            .sum())
    }

    /// The danger head's hidden width, if the weights carry one.
    pub fn danger_hidden(bytes: &[u8]) -> Result<Option<usize>> {
        let st = safetensors::SafeTensors::deserialize(bytes)
            .map_err(|e| candle_core::Error::Msg(format!("weights header: {e}")))?;
        Ok(st.tensor("danger.conv1.weight").ok().map(|t| t.shape()[0]))
    }

    /// The input width `C` of `conv_in` (`[conv, C, 3]`), which names the
    /// feature set the weights were trained on.
    pub fn input_channels(bytes: &[u8]) -> Result<usize> {
        let st = safetensors::SafeTensors::deserialize(bytes)
            .map_err(|e| candle_core::Error::Msg(format!("weights header: {e}")))?;
        st.tensor("conv_in.weight")
            .map(|t| t.shape()[1])
            .map_err(|e| candle_core::Error::Msg(format!("weights: conv_in.weight: {e}")))
    }
}

pub struct Model {
    conv_in: Conv1d,
    res: Vec<(Conv1d, Conv1d)>,
    fc: Linear,
    head: Linear,
    /// The danger head (`danger.conv1`, `danger.conv2`, `danger.out`), when
    /// the weights carry one (`train/train_danger.py`): for every opponent and
    /// tile, how likely that opponent would win on it. It reads the feature
    /// map after the residual blocks beside the input planes, tile by tile.
    danger: Option<[Conv1d; 3]>,
    geo: Geometry,
}

impl Model {
    /// Build the model for `num_players` from a safetensors byte buffer.
    pub fn from_safetensors(bytes: Vec<u8>, num_players: u8) -> Result<Self> {
        let dims = Dims::of(&bytes)?;
        let danger_hidden = Dims::danger_hidden(&bytes)?;
        // The input width says which feature set the weights were trained on.
        let channels = Dims::input_channels(&bytes)?;
        let features = Features::of_channels(num_players, channels).ok_or_else(|| {
            candle_core::Error::Msg(format!(
                "weights: {channels} input channels match no {num_players}-player feature set"
            ))
        })?;
        let dev = Device::Cpu;
        let vb = VarBuilder::from_buffered_safetensors(bytes, candle_core::DType::F32, &dev)?;
        let geo = Geometry::with_features(num_players, features);
        let cfg = Conv1dConfig {
            padding: 1,
            ..Default::default()
        };

        let conv_in = conv1d(geo.channels, dims.conv, 3, cfg, vb.pp("conv_in"))?;
        let mut res = Vec::with_capacity(dims.blocks);
        for i in 0..dims.blocks {
            let b = vb.pp(format!("res.{i}"));
            let c1 = conv1d(dims.conv, dims.conv, 3, cfg, b.pp("conv1"))?;
            let c2 = conv1d(dims.conv, dims.conv, 3, cfg, b.pp("conv2"))?;
            res.push((c1, c2));
        }
        let fc = linear(dims.conv * geo.tile_dim, dims.fc, vb.pp("fc"))?;
        let head = linear(dims.fc, geo.action_space, vb.pp("head"))?;
        let danger = match danger_hidden {
            Some(hidden) => {
                let conv = |k: usize| Conv1dConfig {
                    padding: k / 2,
                    ..Default::default()
                };
                let opponents = num_players as usize - 1;
                Some([
                    conv1d(
                        dims.conv + geo.channels,
                        hidden,
                        7,
                        conv(7),
                        vb.pp("danger.conv1"),
                    )?,
                    conv1d(hidden, hidden / 2, 3, conv(3), vb.pp("danger.conv2"))?,
                    conv1d(hidden / 2, opponents, 1, conv(1), vb.pp("danger.out"))?,
                ])
            }
            None => None,
        };

        Ok(Self {
            conv_in,
            res,
            fc,
            head,
            danger,
            geo,
        })
    }

    pub fn geometry(&self) -> Geometry {
        self.geo
    }

    /// Does this model carry a danger head?
    pub fn has_danger(&self) -> bool {
        self.danger.is_some()
    }

    /// One flattened `[C*T]` observation as `[1, C, T]`, and the feature map
    /// after the residual blocks `[1, conv, T]` — what both heads read.
    fn features(&self, obs: &[f32]) -> Result<(Tensor, Tensor)> {
        let dev = Device::Cpu;
        let input = Tensor::from_vec(
            obs.to_vec(),
            (1, self.geo.channels, self.geo.tile_dim),
            &dev,
        )?;
        let mut x = self.conv_in.forward(&input)?.relu()?;
        for (c1, c2) in &self.res {
            let y = c1.forward(&x)?.relu()?;
            let y = c2.forward(&y)?;
            x = y.add(&x)?.relu()?;
        }
        Ok((input, x))
    }

    /// Forward one flattened `[C*T]` observation to action logits `[A]`.
    pub fn forward_logits(&self, obs: &[f32]) -> Result<Vec<f32>> {
        let (_, x) = self.features(obs)?;
        let x = self.fc.forward(&x.flatten_from(1)?)?.relu()?;
        let logits = self.head.forward(&x)?;
        logits.squeeze(0)?.to_vec1::<f32>()
    }

    /// The danger head's probabilities for one observation: `tile_dim` per
    /// opponent in relative order (shimocha first), as `crate::danger` lays
    /// out the labels. `None` without a danger head.
    pub fn forward_danger(&self, obs: &[f32]) -> Result<Option<Vec<f32>>> {
        let Some([conv1, conv2, out]) = &self.danger else {
            return Ok(None);
        };
        let (input, feat) = self.features(obs)?;
        let h = conv1.forward(&Tensor::cat(&[&feat, &input], 1)?)?.relu()?;
        let h = conv2.forward(&h)?.relu()?;
        let p = candle_nn::ops::sigmoid(&out.forward(&h)?)?;
        Ok(Some(p.flatten_all()?.to_vec1::<f32>()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled weights read back as the size they were trained at.
    #[test]
    fn bundled_weights_report_their_own_size() {
        let dims = Dims::of(crate::defaults::WEIGHTS_3P).unwrap();
        assert_eq!(
            dims,
            Dims {
                conv: CONV,
                blocks: BLOCKS,
                fc: FC
            }
        );
    }

    /// Mora is the larger size on the V2 planes, and loads as a V2 model.
    #[test]
    fn mora_weights_load_as_v2() {
        let dims = Dims::of(crate::defaults::WEIGHTS_MORA_3P).unwrap();
        assert_eq!(
            dims,
            Dims {
                conv: 160,
                blocks: 10,
                fc: 768
            }
        );
        let model = Model::from_safetensors(crate::defaults::WEIGHTS_MORA_3P.to_vec(), 3).unwrap();
        assert_eq!(model.geometry().features, Features::V2);
    }

    /// Mora 4p: the same V2 planes for four players, at the size Mokora 4p had.
    #[test]
    fn mora_4p_weights_load_as_v2() {
        let dims = Dims::of(crate::defaults::WEIGHTS_MORA_4P).unwrap();
        assert_eq!(
            dims,
            Dims {
                conv: 192,
                blocks: 12,
                fc: 768
            }
        );
        let model = Model::from_safetensors(crate::defaults::WEIGHTS_MORA_4P.to_vec(), 4).unwrap();
        assert_eq!(model.geometry().features, Features::V2);
    }
}
