/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Set to "true" to show the style preview button in production builds */
  readonly VITE_SHOW_PREVIEW?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

declare global {
  interface Window {
    NL_TOKEN?: string
    NL_PORT?: number
    NL_MODE?: string
    NL_PATH?: string
    NL_OS?: string
    NL_PID?: string
  }
}

export {}
