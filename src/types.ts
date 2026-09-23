// Mirrors the Rust types in src-tauri/src/manager.rs and profiles.rs.

export interface Profile {
  id: string;
  name: string;
  description: string;
  fps: number;
  opacity: number;
}

export interface SourceView {
  id: string;
  label: string;
  profile_id: string;
  running: boolean;
  /** False while the window picker is still open. */
  started: boolean;
  /** Reopening skips the window picker. */
  remembered: boolean;
  /** 0.2 – 1.0 */
  opacity: number;
}

export interface StateView {
  profiles: Profile[];
  running: SourceView[];
  saved: SourceView[];
}
