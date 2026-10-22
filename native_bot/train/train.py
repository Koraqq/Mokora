#!/usr/bin/env python
"""Behavior-cloning trainer for the Mokora built-in bot.

Reads the Rust extractor's `(obs u8, action u16, mask u8)` shards, trains a small
1-D ResNet CNN with masked cross-entropy, then folds BatchNorm into the
preceding convolutions and exports a `.safetensors` file whose layout the Rust
`native_bot::model` (candle) loader reads directly.

Usage:
  python train.py --data out/p4 --out ../weights/base4p.safetensors \
      --epochs 8 --batch 1024 --lr 1e-3

A dataset larger than RAM trains with `--stream-shards K`: only K shards are
in memory at a time, shuffled together, and the validation set is whole
held-out shards instead of a random slice of samples. Every streamed batch is
full: the samples left over at the end of a group carry into the next.

Before export, every BatchNorm's running statistics are recomputed as a plain
average over `--bn-recal-batches` batches of training data. The moving average
training leaves behind can be thrown far off by one unlucky batch (a tiny
leftover batch did exactly that once: val top-1 fell from 79.5% to 44% between
two epochs while the training loss stayed normal), and the exported weights
fold those statistics in for good.

Progress is one line rewritten every couple of seconds: the epoch, a bar, the
running loss, and the time left for the epoch and for the whole run. After
every epoch the model, optimizer and schedule are saved to `<out>.ckpt`;
`--resume` picks a run up from there after it was stopped. The summary lines
(model, each epoch's losses, export) are also appended to `<out>.log`.
"""
import argparse
import json
import os
import sys
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from safetensors.torch import save_file

# Default architecture — the original built-in weights' size, mirrored by the
# constants in native_bot::model. `--conv/--blocks/--fc` train another size;
# the Rust loader reads the size back off the exported tensors.
CONV = 64
BLOCKS = 3
FC = 256


# --------------------------------------------------------------------------- #
# Data
# --------------------------------------------------------------------------- #
def load_meta(data_dir):
    return json.load(open(os.path.join(data_dir, "meta.json")))


def load_shards(data_dir, meta, shard_ids):
    """Load the given shards into one set of arrays."""
    obs_len = meta["obs_len"]
    A = meta["action_space"]
    counts = meta["shard_samples"]
    n = sum(counts[i] for i in shard_ids)

    obs = np.empty((n, obs_len), dtype=np.uint8)
    act = np.empty((n,), dtype=np.int64)
    msk = np.empty((n, A), dtype=np.uint8)

    off = 0
    for i in shard_ids:
        c = counts[i]
        if c == 0:
            continue
        o = np.fromfile(os.path.join(data_dir, f"{i}.obs"), dtype=np.uint8)
        a = np.fromfile(os.path.join(data_dir, f"{i}.act"), dtype="<u2")
        m = np.fromfile(os.path.join(data_dir, f"{i}.msk"), dtype=np.uint8)
        assert o.size == c * obs_len, f"shard {i} obs size mismatch"
        assert a.size == c and m.size == c * A, f"shard {i} act/msk size mismatch"
        obs[off:off + c] = o.reshape(c, obs_len)
        act[off:off + c] = a.astype(np.int64)
        msk[off:off + c] = m.reshape(c, A)
        off += c
    assert off == n, (off, n)
    return obs, act, msk


def load_dataset(data_dir):
    meta = load_meta(data_dir)
    obs, act, msk = load_shards(data_dir, meta, range(len(meta["shard_samples"])))
    return meta, obs, act, msk


# --------------------------------------------------------------------------- #
# Model
# --------------------------------------------------------------------------- #
class ResBlock(nn.Module):
    def __init__(self, ch):
        super().__init__()
        self.conv1 = nn.Conv1d(ch, ch, 3, padding=1, bias=False)
        self.bn1 = nn.BatchNorm1d(ch)
        self.conv2 = nn.Conv1d(ch, ch, 3, padding=1, bias=False)
        self.bn2 = nn.BatchNorm1d(ch)

    def forward(self, x):
        y = F.relu(self.bn1(self.conv1(x)))
        y = self.bn2(self.conv2(y))
        return F.relu(y + x)


class Net(nn.Module):
    def __init__(self, c_in, tile_dim, n_actions, conv=CONV, blocks=BLOCKS, fc=FC):
        super().__init__()
        self.conv_in = nn.Conv1d(c_in, conv, 3, padding=1, bias=False)
        self.bn_in = nn.BatchNorm1d(conv)
        self.res = nn.ModuleList([ResBlock(conv) for _ in range(blocks)])
        self.fc = nn.Linear(conv * tile_dim, fc)
        self.head = nn.Linear(fc, n_actions)

    def forward(self, x):
        x = F.relu(self.bn_in(self.conv_in(x)))
        for r in self.res:
            x = r(x)
        x = torch.flatten(x, 1)
        x = F.relu(self.fc(x))
        return self.head(x)


# --------------------------------------------------------------------------- #
# BatchNorm folding (Conv1d(bias=False) -> BN  ==>  Conv1d(bias=True))
# --------------------------------------------------------------------------- #
def fold(conv_w, bn):
    scale = bn.weight / torch.sqrt(bn.running_var + bn.eps)
    w = conv_w * scale.reshape(-1, 1, 1)
    b = bn.bias - bn.running_mean * scale
    return w.detach(), b.detach()


def folded_state_dict(model):
    sd = {}
    w, b = fold(model.conv_in.weight, model.bn_in)
    sd["conv_in.weight"], sd["conv_in.bias"] = w, b
    for i, r in enumerate(model.res):
        w1, b1 = fold(r.conv1.weight, r.bn1)
        w2, b2 = fold(r.conv2.weight, r.bn2)
        sd[f"res.{i}.conv1.weight"], sd[f"res.{i}.conv1.bias"] = w1, b1
        sd[f"res.{i}.conv2.weight"], sd[f"res.{i}.conv2.bias"] = w2, b2
    sd["fc.weight"], sd["fc.bias"] = model.fc.weight.detach(), model.fc.bias.detach()
    sd["head.weight"], sd["head.bias"] = model.head.weight.detach(), model.head.bias.detach()
    return {k: v.contiguous().cpu() for k, v in sd.items()}


def folded_forward(sd, x, tile_dim):
    """Pure conv+linear forward (no BN) — used to verify folding parity."""
    x = F.relu(F.conv1d(x, sd["conv_in.weight"], sd["conv_in.bias"], padding=1))
    i = 0
    while f"res.{i}.conv1.weight" in sd:
        y = F.relu(F.conv1d(x, sd[f"res.{i}.conv1.weight"], sd[f"res.{i}.conv1.bias"], padding=1))
        y = F.conv1d(y, sd[f"res.{i}.conv2.weight"], sd[f"res.{i}.conv2.bias"], padding=1)
        x = F.relu(y + x)
        i += 1
    x = torch.flatten(x, 1)
    x = F.relu(F.linear(x, sd["fc.weight"], sd["fc.bias"]))
    return F.linear(x, sd["head.weight"], sd["head.bias"])


LOG_PATH = None


def note(msg):
    """Print a summary line and append it, timestamped, to `<out>.log`."""
    print(msg)
    if LOG_PATH:
        with open(LOG_PATH, "a", encoding="utf-8") as f:
            f.write(time.strftime("%Y-%m-%d %H:%M:%S  ") + msg + "\n")


def hms(secs):
    secs = max(0, int(secs))
    return f"{secs // 3600}:{secs // 60 % 60:02d}:{secs % 60:02d}"


class Progress:
    """One progress line, rewritten in place at most every `every` seconds."""

    def __init__(self, epochs, batches_per_epoch, every=2.0):
        self.epochs = epochs
        self.per_epoch = max(1, batches_per_epoch)
        self.every = every
        self.last = 0.0
        # Batches already done before this process started (a resumed run).
        self.resumed_at = 0

    def update(self, ep, nb, loss, run_start, done_before, force=False):
        now = time.time()
        if not force and now - self.last < self.every:
            return
        self.last = now
        frac = min(1.0, nb / self.per_epoch)
        done = done_before + nb
        total = self.epochs * self.per_epoch
        rate = (now - run_start) / max(1, done - self.resumed_at)
        epoch_left = (self.per_epoch - nb) * rate
        run_left = (total - done) * rate
        bar = "#" * int(frac * 24) + "." * (24 - int(frac * 24))
        finish = time.strftime("%m/%d %H:%M", time.localtime(now + run_left))
        sys.stdout.write(
            f"\r  epoch {ep + 1}/{self.epochs} [{bar}] {frac * 100:5.1f}%  loss {loss:.4f}  "
            f"epoch left {hms(epoch_left)}  total left {hms(run_left)}  done ~{finish}   "
        )
        sys.stdout.flush()


# --------------------------------------------------------------------------- #
# Train
# --------------------------------------------------------------------------- #
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--batch", type=int, default=1024)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--val-frac", type=float, default=0.02)
    ap.add_argument("--conv", type=int, default=CONV, help="backbone channels")
    ap.add_argument("--blocks", type=int, default=BLOCKS, help="residual blocks")
    ap.add_argument("--fc", type=int, default=FC, help="hidden fully-connected width")
    ap.add_argument("--stream-shards", type=int, default=0,
                    help="keep only this many shards in memory at a time (0 = load everything)")
    ap.add_argument("--bn-recal-batches", type=int, default=200,
                    help="training batches to recompute BatchNorm statistics over before export (0 = keep the moving average)")
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    ap.add_argument("--resume", action="store_true",
                    help="continue from <out>.ckpt, saved after every epoch")
    args = ap.parse_args()
    ckpt_path = args.out + ".ckpt"
    global LOG_PATH
    LOG_PATH = args.out + ".log"

    torch.manual_seed(0)
    np.random.seed(0)

    meta = load_meta(args.data)
    C, T, A = meta["channels"], meta["tile_dim"], meta["action_space"]
    obs_scale = meta["obs_scale"]
    counts = meta["shard_samples"]

    if args.stream_shards:
        # Hold out whole shards (so whole games) for validation, and stream
        # the rest through memory a group of shards at a time.
        shard_ids = [i for i, c in enumerate(counts) if c > 0]
        np.random.shuffle(shard_ids)
        n_val_shards = max(1, round(len(shard_ids) * args.val_frac))
        val_shards, train_shards = shard_ids[:n_val_shards], shard_ids[n_val_shards:]
        val_obs, val_act, val_msk = load_shards(args.data, meta, val_shards)
        n_train = sum(counts[i] for i in train_shards)
        note(f"streaming {n_train} train samples from {len(train_shards)} shards, "
              f"{args.stream_shards} at a time; {len(val_act)} val samples "
              f"from {len(val_shards)} shards  C={C} T={T} A={A}  device={args.device}")
    else:
        obs, act, msk = load_shards(args.data, meta, range(len(counts)))
        n = meta["total_samples"]
        note(f"loaded {n} samples  C={C} T={T} A={A}  device={args.device}")
        perm = np.random.permutation(n)
        n_val = int(n * args.val_frac)
        val_idx, train_idx = perm[:n_val], perm[n_val:]
        n_train = len(train_idx)

    dev = torch.device(args.device)
    model = Net(C, T, A, conv=args.conv, blocks=args.blocks, fc=args.fc).to(dev)
    note(f"model: conv={args.conv} blocks={args.blocks} fc={args.fc}  "
          f"params={sum(p.numel() for p in model.parameters()):,}")
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    steps = args.epochs * (n_train // args.batch + 1)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=steps, eta_min=args.lr * 0.02)

    def tensors(obs, act, msk):
        # uint8, on CPU; slice+move per batch
        return torch.from_numpy(obs), torch.from_numpy(act), torch.from_numpy(msk)

    def batch(data, idx):
        obs_t, act_t, msk_t = data
        xb = obs_t[idx].to(dev, non_blocking=True).float().div_(255.0).mul_(obs_scale)
        xb = xb.view(-1, C, T)
        ab = act_t[idx].to(dev, non_blocking=True)
        mb = msk_t[idx].to(dev, non_blocking=True).bool()
        return xb, ab, mb

    if args.stream_shards:
        val_data = tensors(val_obs, val_act, val_msk)
        val_idx = np.arange(len(val_act))
    else:
        val_data = train_data = tensors(obs, act, msk)

    def train_groups():
        """(tensors, shuffled indices) for one epoch's worth of training data."""
        if not args.stream_shards:
            np.random.shuffle(train_idx)
            yield train_data, train_idx
            return
        order = list(train_shards)
        np.random.shuffle(order)
        # Only full batches: what is left of a group joins the next one, and
        # the last few samples of the epoch (less than a batch) are dropped.
        # A tiny batch's statistics would knock BatchNorm's running averages.
        carry = None
        for g in range(0, len(order), args.stream_shards):
            o, a, m = load_shards(args.data, meta, order[g:g + args.stream_shards])
            if carry is not None:
                o = np.concatenate([carry[0], o])
                a = np.concatenate([carry[1], a])
                m = np.concatenate([carry[2], m])
            perm = np.random.permutation(len(a))
            full = len(perm) // args.batch * args.batch
            rest = perm[full:]
            carry = (o[rest], a[rest], m[rest])
            yield tensors(o, a, m), perm[:full]
            # Let go of this group before the next one is read: the caller drops
            # its own reference after the group, so only one group is in memory
            # at a time (4-player V2 groups are tens of GB).
            o = a = m = None

    def recalibrate_bn(n_batches):
        """Recompute every BatchNorm's running mean / variance as a plain
        average over `n_batches` full training batches (momentum=None makes
        PyTorch average instead of decay), replacing the moving average."""
        bns = [mod for mod in model.modules() if isinstance(mod, nn.BatchNorm1d)]
        for bn in bns:
            bn.reset_running_stats()
            bn.momentum = None
        model.train()
        seen = 0
        with torch.no_grad():
            for data, order in train_groups():
                for s in range(0, len(order) - args.batch + 1, args.batch):
                    xb, _, _ = batch(data, order[s:s + args.batch])
                    model(xb)
                    seen += 1
                    if seen >= n_batches:
                        break
                del data, order
                if seen >= n_batches:
                    break
        model.eval()

    def evaluate():
        model.eval()
        correct = tot = 0
        vloss = 0.0
        with torch.no_grad():
            for s in range(0, len(val_idx), args.batch):
                idx = val_idx[s:s + args.batch]
                xb, ab, mb = batch(val_data, idx)
                logits = model(xb).masked_fill(~mb, -1e9)
                vloss += F.cross_entropy(logits, ab, reduction="sum").item()
                correct += (logits.argmax(1) == ab).sum().item()
                tot += len(idx)
        return vloss / max(tot, 1), correct / max(tot, 1)

    start_ep = 0
    if args.resume and os.path.exists(ckpt_path):
        ck = torch.load(ckpt_path, map_location=dev, weights_only=False)
        model.load_state_dict(ck["model"])
        opt.load_state_dict(ck["opt"])
        sched.load_state_dict(ck["sched"])
        np.random.set_state(ck["np_rng"])
        torch.set_rng_state(ck["torch_rng"].cpu())
        start_ep = ck["epoch"]
        note(f"resumed from {ckpt_path}: {start_ep}/{args.epochs} epochs done")

    progress = Progress(args.epochs, n_train // args.batch + (0 if args.stream_shards else 1))
    progress.resumed_at = start_ep * progress.per_epoch
    run_start = time.time()
    for ep in range(start_ep, args.epochs):
        model.train()
        t0 = time.time()
        run = 0.0
        nb = 0
        for data, order in train_groups():
            for s in range(0, len(order), args.batch):
                idx = order[s:s + args.batch]
                xb, ab, mb = batch(data, idx)
                logits = model(xb).masked_fill(~mb, -1e9)
                loss = F.cross_entropy(logits, ab)
                opt.zero_grad(set_to_none=True)
                loss.backward()
                opt.step()
                sched.step()
                run += loss.item()
                nb += 1
                progress.update(ep, nb, run / nb, run_start, ep * progress.per_epoch)
            del data, order
        progress.update(ep, nb, run / max(nb, 1), run_start, ep * progress.per_epoch, force=True)
        print()
        vloss, vacc = evaluate()
        note(f"epoch {ep+1}/{args.epochs}  train_loss={run/max(nb,1):.4f}  "
              f"val_loss={vloss:.4f}  val_top1={vacc*100:.2f}%  ({time.time()-t0:.1f}s)")
        torch.save({
            "model": model.state_dict(),
            "opt": opt.state_dict(),
            "sched": sched.state_dict(),
            "np_rng": np.random.get_state(),
            "torch_rng": torch.get_rng_state(),
            "epoch": ep + 1,
        }, ckpt_path)

    if args.bn_recal_batches > 0:
        recalibrate_bn(args.bn_recal_batches)
        vloss, vacc = evaluate()
        note(f"BN statistics recomputed over {args.bn_recal_batches} batches  "
              f"val_loss={vloss:.4f}  val_top1={vacc*100:.2f}%")

    # Fold BN and verify parity, then export. Folding is analytically exact;
    # float32 rounding through the deep net leaves a small absolute logit diff,
    # so we check that the *decisions* (argmax) still agree, which is what the
    # bot actually uses.
    model.eval()
    sd = folded_state_dict(model)
    # Full float32 for the comparison: PyTorch runs CUDA convolutions in TF32
    # by default, whose rounding alone moved 0.02% of a 7.8M model's choices
    # (logits reach ~1e5) — a gap the CPU inference the bot does never sees.
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cuda.matmul.allow_tf32 = False
    with torch.no_grad():
        xb, _, mb = batch(val_data, val_idx[: min(4096, len(val_idx))])
        ref = model(xb)
        fol = folded_forward({k: v.to(dev) for k, v in sd.items()}, xb, T)
        max_diff = (ref - fol).abs().max().item()
        # Agreement over legal actions (matches inference masking).
        ref_a = ref.masked_fill(~mb, -1e9).argmax(1)
        fol_a = fol.masked_fill(~mb, -1e9).argmax(1)
        agree = (ref_a == fol_a).float().mean().item()
    note(f"BN-fold parity: max|diff|={max_diff:.2e}  argmax_agreement={agree*100:.2f}%")
    assert agree > 0.995, "BN folding changed too many decisions"

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    save_file(
        sd,
        args.out,
        metadata={
            "num_players": str(meta["num_players"]),
            "channels": str(C),
            "tile_dim": str(T),
            "action_space": str(A),
            "conv": str(args.conv),
            "blocks": str(args.blocks),
            "fc": str(args.fc),
            "obs_scale": str(obs_scale),
        },
    )
    note(f"wrote {args.out}")


if __name__ == "__main__":
    main()
