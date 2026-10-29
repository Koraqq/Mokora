<p align="center">
  <img alt="Mokora" src="assets/logo/mokora-icon.png" width="120">
</p>

<h1 align="center">Mokora</h1>

<p align="center">
  A real-time Mahjong Soul assistant with a built-in AI model.<br/>
  <a href="./README.zh-TW.md">繁體中文</a><br/>
  <a href="./README.ja.md">日本語</a><br/>
  <a href="https://discord.gg/Ta6bqH6NwN">Discord</a>
</p>

---

Mokora watches your Mahjong Soul game and shows, in a draggable HUD, the
shanten, waits, win and tenpai rates, deal-in risk against each opponent and
the recommended discard. The model is embedded: nothing to install.

## Features

- Live HUD with a draggable, resizable layout
- Built-in bot for four-player and three-player games; external mjai bots can be plugged in
- Browser capture or proxy capture
- Several accounts, each with its own history
- Game history with statistics, and whole-game review
- Autoplay, off by default
- English, 日本語, 繁體中文, 简体中文

> For learning from your own games. Using an assistant in ranked play may
> breach the game's terms of use; any consequence is yours.

## Linux

Ubuntu 22.04 or newer, Kali Linux and other Debian-based systems with
WebKitGTK 4.1 (everything below is the same on all of them).

Install from the `.deb` (it pulls in what it needs):

```sh
sudo apt update
sudo apt install ./Mokora_*_amd64.deb
mokora
```

Or run the `.AppImage`, which needs no installation:

```sh
sudo apt install libfuse2        # Ubuntu 24.04 and newer: libfuse2t64
chmod +x Mokora_*_amd64.AppImage
./Mokora_*_amd64.AppImage
```

Remove it with `sudo apt remove mokora`. Settings and history are kept in
`~/.config/mokora`.

## Licence

Apache License 2.0 — see [LICENSE.txt](./LICENSE.txt) and [NOTICE](./NOTICE).
