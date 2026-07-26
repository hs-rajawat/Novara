<p align="center">
  <h1 align="center">🎮 NOVARA</h1>

  <p align="center">
    A modern, local-first game library manager built for Windows.
  </p>

  <p align="center">
    <strong>Fast • Beautiful • Private • Open Source</strong>
  </p>
</p>

---

## ✨ Why NOVARA?

Most launchers only care about games you bought through them.

NOVARA is different.

Whether your games come from Steam, Epic Games, or are manually installed, NOVARA lets you organise them in one clean library.

**No account required.  
No cloud dependency.  
Just your games.**

---

## 🔒 Privacy

NOVARA is built with a local-first philosophy.

- No account required
- No telemetry
- No cloud dependency
- Metadata networking can be disabled
- Your library stays on your computer

---

## 📂 First-Class Support for Manual Games

Most launchers either ignore manually installed games or treat them as shortcuts.

NOVARA gives them the same experience as launcher-managed games.

**Manual games support:**

  **✨ Features**
  
- 🎮 Automatic library integration
- 🚀 One-click launching
- 📊 Playtime tracking
- 📈 Progress & completion tracking
- 💾 Save backup & restore
- 🎨 Metadata & artwork (Steam today, expanding to other sources)
- 🔍 Installation integrity verification
- ⚡ Fast native performance
- 🔒 Fully local-first — no launcher required

Whether a game came from Steam, itch.io, GOG installers, old DVDs, or any standalone executable, it belongs in your library.

---

## 🖼 Preview

> Screenshots coming soon.

---

## 🛠 Tech Stack

### Frontend

- React
- TypeScript
- Vite

### Backend

- Rust
- Tauri v2
- SQLite

---

## 📦 Development Setup

```bash
git clone https://github.com/hs-rajawat/NOVARA.git
cd NOVARA

npm install
npm run build

cargo test
npm run tauri dev
```

> **Note**
>
> On a fresh clone, run `npm run build` before running `cargo test` or `npm run tauri dev`.
>
> NOVARA uses Tauri's `generate_context!()` macro, which depends on the generated frontend assets in `dist/`. Because `dist/` is gitignored, those assets don't exist until the frontend has been built once.

---

## 📍 Roadmap

- [x] Steam support
- [x] Epic Games support
- [x] Playtime tracking
- [x] Save management
- [x] Installation verification
- [ ] GOG support
- [ ] Ubisoft Connect support
- [ ] EA App support
- [ ] Themes
- [ ] Cloud sync (optional)

---

## 🤝 Contributing

Contributions, bug reports, and feature suggestions are always welcome.

If you'd like to help improve NOVARA, feel free to open an issue or submit a pull request.

---

## ⭐ Support

If you like the project, consider giving it a ⭐.

It helps more people discover NOVARA and motivates future development.

---

## 📄 License

This project is licensed under the **MIT License**.
