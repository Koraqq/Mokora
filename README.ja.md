<p align="center">
  <img alt="Mokora" src="assets/logo/mokora-icon.png" width="120">
</p>

<h1 align="center">Mokora</h1>

<p align="center">
  AI モデルを内蔵した雀魂向けリアルタイム支援ツール。<br/>
  <a href="./README.md">English</a><br/>
  <a href="./README.zh-TW.md">繁體中文</a><br/>
  <a href="https://discord.gg/Ta6bqH6NwN">Discord</a>
</p>

---

Mokora は雀魂の対局を監視し、ドラッグ可能な HUD に向聴数、待ち牌、
和了率・聴牌率、各対戦相手への放銃リスク、おすすめの打牌を表示します。
モデルは内蔵されているため、追加のインストールは不要です。

## 機能

- ドラッグとサイズ変更が可能なリアルタイム HUD
- 四人麻雀・三人麻雀に対応した内蔵ボット。外部の mjai ボットも使用可能
- ブラウザキャプチャまたはプロキシキャプチャ
- 複数アカウントに対応し、アカウントごとに履歴を保存
- 対局履歴、統計、対局全体のレビュー
- 自動打牌機能（初期設定では無効）
- English、日本語、繁體中文、简体中文

> 自分の対局を振り返り、学習するためのツールです。段位戦で支援ツールを
> 使用すると、ゲームの利用規約に違反する可能性があります。その結果については
> ご自身の責任となります。

## Linux

Ubuntu 22.04 以降、Kali Linux、そのほか WebKitGTK 4.1 のある Debian 系で動きます
（以下のコマンドはどれでも同じです）。

`.deb` でインストールします（必要なパッケージも一緒に入ります）：

```sh
sudo apt update
sudo apt install ./Mokora_*_amd64.deb
mokora
```

インストールせず `.AppImage` をそのまま実行することもできます：

```sh
sudo apt install libfuse2        # Ubuntu 24.04 以降は libfuse2t64
chmod +x Mokora_*_amd64.AppImage
./Mokora_*_amd64.AppImage
```

削除は `sudo apt remove mokora`。設定と履歴は `~/.config/mokora` に保存されます。

## ライセンス

Apache License 2.0。詳しくは [LICENSE.txt](./LICENSE.txt) と
[NOTICE](./NOTICE) をご覧ください。
