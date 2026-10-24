# lin_client

Lin's planned desktop/mobile client uses React 19, TypeScript, Vite, and Tauri 2.
The current implementation is the greeting template: it invokes the Rust `greet`
command. Authentication, chat screens, and backend integration are not implemented.

## Development

From `lin_client/`, install dependencies with `npm install`, then run:

```sh
npm run tauri dev
```

This requires Rust and the platform-specific Tauri native dependencies.
`npm run dev` starts Vite alone; the greeting command requires the Tauri runtime.

## Checks

```sh
npm run build
```

From `lin_client/src-tauri/`, run `cargo check` with the native dependencies installed.
The frontend build does not validate the desktop runtime or backend integration.
