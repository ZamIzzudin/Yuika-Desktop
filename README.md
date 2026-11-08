# Yuika Desktop

The desktop shell for [Yuika](https://github.com/ZamIzzudin/Yuika), a Studio
Persona for Agentic AI. It is a thin Tauri 2 client: the shell reuses the
studio web UI and onboards you into host mode (a local studio stack) or
remote mode (your own studio instance, for example one deployed behind
Dokploy or Caddy).

This repository contains only the shell. The studio itself (web app, API,
personas, skills, deployment files) lives in the Yuika repository.

## Prerequisites

- Node.js 22 or newer, npm
- Rust toolchain (for the Tauri shell)
- A checkout of the Yuika studio repository, either:
  - next to this repository (`../your-yui` or `../Yuika`), or
  - anywhere on disk, exposed via `YUIKA_STUDIO_DIR`

The studio checkout provides the web UI (dev server and export build) and,
in host mode, the API dev server that the shell spawns.

## Develop

```bash
npm install
npm run desktop
```

The shell starts, builds onboarding from the studio web dev server
(http://localhost:3000), and in host mode spawns the studio API
(127.0.0.1:3211) from the studio checkout.

## Build (Windows NSIS installer)

```bash
npm run desktop:build
```

The script exports the studio web UI (static) into `dist/web`, bundles it,
and produces the installer under `apps/desktop/src-tauri/target/release/`.

Releases of this repository are desktop releases only; server releases
belong to the Yuika studio repository and drive its update indicator.

## Layout

- `apps/desktop` - Tauri shell (Rust, capabilities, icons, config)
- `scripts` - build/dev helpers that resolve the studio checkout
- `docs/PLAN-DESKTOP.md` - development plan for the shell
