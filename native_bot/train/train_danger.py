#!/usr/bin/env python
"""Train a danger head on top of an exported model, leaving it untouched.

The head predicts, for every tile and every opponent, whether that opponent
would win on it right now (tenpai, waiting on it, not furiten) — the labels
the extractor writes with `--danger` (`native_bot::danger`). It works tile by
tile: it reads the model's feature map after the residual blocks (one column
per tile, which already holds the whole table's context) next to the input
planes themselves (each player's discards, the tiles they let pass…), so the
exact per-tile facts — a tile in an opponent's pond is safe against them —
reach it unblurred. Convolutions along the tile axis (7 wide, so suji and
kabe three tiles away are in view) end in one output per opponent per tile.
A first version on the policy's last hidden layer (`relu(fc)`) lost those
facts and ranked tiles worse than the rule-based risk.

The model itself is frozen and run exactly as exported (BatchNorm already
folded), so the policy in the output file is bit-for-bit the input's: only
the `danger.*` tensors are added.

Usage:
  python train_danger.py --data out/p3v2d --base ../weights/mora3p.safetensors \
      --out out/mora3p-danger.safetensors --epochs 3

Progress, `<out>.ckpt` / `--resume` and `<out>.log` work as in train.py.
Each epoch reports the validation loss, the AUC (how well danger ranks the
tiles opponents really wait on above the rest) and the calibration (mean
predicted probability against the real rate).
"""
import argparse
import os
import sys
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from safetensors.torch import load_file, save_file
from safetensors import safe_open

import train as base


def load_shards(data_dir, meta, shard_ids):
    """Observations and danger labels of the given shards."""
    obs_len, D = meta["obs_len"], meta["danger_len"]
    counts = meta["shard_samples"]
    n = sum(counts[i] for i in shard_ids)
    obs = np.empty((n, obs_len), dtype=np.uint8)
    dgr = np.empty((n, D), dtype=np.uint8)
    off = 0
    for i in shard_ids:
        c = counts[i]
        if c == 0:
            continue
        o = np.fromfile(os.path.join(data_dir, f"{i}.obs"), dtype=np.uint8)
        d = np.fromfile(os.path.join(data_dir, f"{i}.dgr"), dtype=np.uint8)
        assert o.size == c * obs_len and d.size == c * D, f"shard {i} size mismatch"
        obs[off:off + c] = o.reshape(c, obs_len)
        dgr[off:off + c] = d.reshape(c, D)
        off += c
    return obs, dgr


class FoldedTrunk(nn.Module):
    """The exported model up to its last hidden layer, frozen."""

    def __init__(self, sd):
        super().__init__()
        self.sd = {k: nn.Parameter(v, requires_grad=False) for k, v in sd.items()}
        for k, v in self.sd.items():
            self.register_parameter(k.replace(".", "_"), v)

    def forward(self, x):
        """The feature map after the residual blocks, `[B, conv, T]`."""
        sd = self.sd
        x = F.relu(F.conv1d(x, sd["conv_in.weight"], sd["conv_in.bias"], padding=1))
        i = 0
        while f"res.{i}.conv1.weight" in sd:
            y = F.relu(F.conv1d(x, sd[f"res.{i}.conv1.weight"], sd[f"res.{i}.conv1.bias"], padding=1))
            y = F.conv1d(y, sd[f"res.{i}.conv2.weight"], sd[f"res.{i}.conv2.bias"], padding=1)
            x = F.relu(y + x)
            i += 1
        return x


class DangerHead(nn.Module):
    """Tile-wise head: [features ; input planes] -> one logit per opponent
    per tile, laid out opponent-major like the labels."""

    def __init__(self, in_ch, hidden, opponents):
        super().__init__()
        self.conv1 = nn.Conv1d(in_ch, hidden, 7, padding=3)
        self.conv2 = nn.Conv1d(hidden, hidden // 2, 3, padding=1)
        self.out = nn.Conv1d(hidden // 2, opponents, 1)

    def forward(self, feat, x):
        h = F.relu(self.conv1(torch.cat([feat, x], 1)))
        h = F.relu(self.conv2(h))
        return torch.flatten(self.out(h), 1)


def auc(scores, labels):
    """Area under the ROC curve, by ranks (ties count half)."""
    order = torch.argsort(scores)
    ranks = torch.empty_like(scores)
    ranks[order] = torch.arange(1, len(scores) + 1, device=scores.device, dtype=scores.dtype)
    pos = labels.bool()
    n_pos = pos.sum().item()
    n_neg = len(labels) - n_pos
    if n_pos == 0 or n_neg == 0:
        return float("nan")
    return ((ranks[pos].sum().item() - n_pos * (n_pos + 1) / 2) / (n_pos * n_neg))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", required=True, help="extract output written with --danger")
    ap.add_argument("--base", required=True, help="the exported model to put the head on")
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=int, default=3)
    ap.add_argument("--batch", type=int, default=2048)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--hidden", type=int, default=128, help="the head's channels")
    ap.add_argument("--val-frac", type=float, default=0.02)
    ap.add_argument("--stream-shards", type=int, default=16)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    ap.add_argument("--resume", action="store_true")
    args = ap.parse_args()
    ckpt_path = args.out + ".ckpt"
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    base.LOG_PATH = args.out + ".log"
    note = base.note

    torch.manual_seed(0)
    np.random.seed(0)

    meta = base.load_meta(args.data)
    C, T, D = meta["channels"], meta["tile_dim"], meta.get("danger_len", 0)
    obs_scale = meta["obs_scale"]
    assert D > 0, f"{args.data} has no danger labels: extract it with --danger"
    # Any danger head the base already carries is replaced, not kept.
    sd = {k: v for k, v in load_file(args.base).items() if not k.startswith("danger.")}
    with safe_open(args.base, "pt") as f:
        base_meta = f.metadata() or {}
    assert sd["conv_in.weight"].shape[1] == C, (
        f"{args.base} reads {sd['conv_in.weight'].shape[1]} channels, the data has {C}")
    conv = sd["conv_in.weight"].shape[0]
    opponents = D // T

    counts = meta["shard_samples"]
    shard_ids = [i for i, c in enumerate(counts) if c > 0]
    np.random.shuffle(shard_ids)
    n_val = max(1, round(len(shard_ids) * args.val_frac))
    val_shards, train_shards = shard_ids[:n_val], shard_ids[n_val:]
    val_obs, val_dgr = load_shards(args.data, meta, val_shards)
    n_train = sum(counts[i] for i in train_shards)
    note(f"danger head on {args.base}: {n_train} train / {len(val_dgr)} val samples, "
         f"{D} labels each, positive rate {val_dgr.mean() * 100:.2f}%  device={args.device}")

    dev = torch.device(args.device)
    trunk = FoldedTrunk({k: v.to(dev) for k, v in sd.items()}).to(dev).eval()
    head = DangerHead(conv + C, args.hidden, opponents).to(dev)
    note(f"head: [{conv} features + {C} planes] x {T} tiles -> {opponents} per tile  "
         f"params={sum(p.numel() for p in head.parameters()):,}")
    opt = torch.optim.AdamW(head.parameters(), lr=args.lr, weight_decay=1e-4)
    steps = args.epochs * (n_train // args.batch + 1)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=steps, eta_min=args.lr * 0.02)

    def to_x(obs):
        x = torch.from_numpy(obs).to(dev, non_blocking=True).float().div_(255.0).mul_(obs_scale)
        return x.view(-1, C, T)

    def groups():
        order = list(train_shards)
        np.random.shuffle(order)
        for g in range(0, len(order), args.stream_shards):
            o, d = load_shards(args.data, meta, order[g:g + args.stream_shards])
            perm = np.random.permutation(len(d))
            yield o, d, perm
            # Let go of this group before the next one is read; the caller drops
            # its own references after the group, so only one group is in memory
            # (4-player V2 groups are tens of GB).
            o = d = perm = None

    def evaluate():
        head.eval()
        loss, probs, labels = 0.0, [], []
        with torch.no_grad():
            for s in range(0, len(val_dgr), args.batch):
                x = to_x(val_obs[s:s + args.batch])
                y = torch.from_numpy(val_dgr[s:s + args.batch]).to(dev).float()
                z = head(trunk(x), x)
                loss += F.binary_cross_entropy_with_logits(z, y, reduction="sum").item()
                probs.append(torch.sigmoid(z).flatten())
                labels.append(y.flatten())
        p, y = torch.cat(probs), torch.cat(labels)
        return loss / y.numel(), auc(p, y), p.mean().item(), y.mean().item()

    start_ep = 0
    if args.resume and os.path.exists(ckpt_path):
        ck = torch.load(ckpt_path, map_location=dev, weights_only=False)
        head.load_state_dict(ck["head"])
        opt.load_state_dict(ck["opt"])
        sched.load_state_dict(ck["sched"])
        np.random.set_state(ck["np_rng"])
        torch.set_rng_state(ck["torch_rng"].cpu())
        start_ep = ck["epoch"]
        note(f"resumed from {ckpt_path}: {start_ep}/{args.epochs} epochs done")

    progress = base.Progress(args.epochs, n_train // args.batch + 1)
    progress.resumed_at = start_ep * progress.per_epoch
    run_start = time.time()
    for ep in range(start_ep, args.epochs):
        head.train()
        t0, run, nb = time.time(), 0.0, 0
        for o, d, perm in groups():
            for s in range(0, len(perm), args.batch):
                idx = perm[s:s + args.batch]
                x = to_x(o[idx])
                y = torch.from_numpy(d[idx]).to(dev).float()
                with torch.no_grad():
                    feat = trunk(x)
                loss = F.binary_cross_entropy_with_logits(head(feat, x), y)
                opt.zero_grad(set_to_none=True)
                loss.backward()
                opt.step()
                sched.step()
                run += loss.item()
                nb += 1
                progress.update(ep, nb, run / nb, run_start, ep * progress.per_epoch)
            del o, d, perm
        progress.update(ep, nb, run / max(nb, 1), run_start, ep * progress.per_epoch, force=True)
        print()
        vloss, vauc, pmean, ymean = evaluate()
        note(f"epoch {ep + 1}/{args.epochs}  train_loss={run / max(nb, 1):.5f}  val_loss={vloss:.5f}  "
             f"val_auc={vauc:.4f}  mean_pred={pmean * 100:.2f}% vs real={ymean * 100:.2f}%  "
             f"({time.time() - t0:.1f}s)")
        torch.save({
            "head": head.state_dict(),
            "opt": opt.state_dict(),
            "sched": sched.state_dict(),
            "np_rng": np.random.get_state(),
            "torch_rng": torch.get_rng_state(),
            "epoch": ep + 1,
        }, ckpt_path)

    out = dict(sd)
    for name in ("conv1", "conv2", "out"):
        layer = getattr(head, name)
        out[f"danger.{name}.weight"] = layer.weight.detach().contiguous().cpu()
        out[f"danger.{name}.bias"] = layer.bias.detach().contiguous().cpu()
    save_file(out, args.out, metadata={**base_meta, "danger": str(D), "danger_hidden": str(args.hidden)})
    note(f"wrote {args.out} (the policy tensors are {args.base}'s, unchanged)")


if __name__ == "__main__":
    main()
