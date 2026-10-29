<p align="center">
  <img alt="Mokora" src="assets/logo/mokora-icon.png" width="120">
</p>

<h1 align="center">Mokora</h1>

<p align="center">
  內建 AI 模型的雀魂即時輔助工具。<br/>
  <a href="./README.md">English</a><br/>
  <a href="./README.ja.md">日本語</a><br/>
  <a href="https://discord.gg/Ta6bqH6NwN">Discord</a>
</p>

---

Mokora 監看你的雀魂對局，在可拖曳的 HUD 上顯示向聽、待牌、和牌率、聽牌率、
對各家的放銃風險和推薦切牌。模型直接內建，不用另外安裝。

## 功能

- 即時 HUD，版面可拖曳、可縮放
- 內建 bot，四麻三麻都能用；也可以接外部的 mjai bot
- 瀏覽器擷取或代理擷取
- 多帳號，各有自己的歷史紀錄
- 對局歷史與統計、整場檢討
- 自動打牌，預設關閉
- English、日本語、繁體中文、简体中文

> 給你檢討自己牌局用的。在排位賽使用輔助工具可能違反遊戲條款，後果自行承擔。

## Linux

支援 Ubuntu 22.04 以上、Kali Linux，以及其他有 WebKitGTK 4.1 的 Debian 系統
（以下指令在這些系統上都一樣）。

用 `.deb` 安裝（需要的套件會一併裝好）：

```sh
sudo apt update
sudo apt install ./Mokora_*_amd64.deb
mokora
```

或是直接執行 `.AppImage`，不用安裝：

```sh
sudo apt install libfuse2        # Ubuntu 24.04 以上要裝 libfuse2t64
chmod +x Mokora_*_amd64.AppImage
./Mokora_*_amd64.AppImage
```

移除用 `sudo apt remove mokora`。設定與紀錄放在 `~/.config/mokora`。

## 授權

Apache License 2.0，見 [LICENSE.txt](./LICENSE.txt) 與 [NOTICE](./NOTICE)。
