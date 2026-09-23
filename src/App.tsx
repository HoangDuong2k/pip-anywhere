import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SourceView, StateView } from "./types";
import "./App.css";

function App() {
  const [state, setState] = useState<StateView | null>(null);
  const [profileId, setProfileId] = useState("default");
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ id: string; label: string } | null>(null);

  useEffect(() => {
    invoke<StateView>("get_state").then(setState);
    const unlisten = [
      listen<StateView>("state-changed", (e) => setState(e.payload)),
      listen<string>("pip-error", (e) => setError(e.payload)),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, []);

  // Update the slider immediately; the PiP follows through the backend.
  const setOpacity = (id: string, opacity: number) => {
    setState((s) => s && { ...s, running: s.running.map((r) => (r.id === id ? { ...r, opacity } : r)) });
    invoke("set_opacity", { id, opacity }).catch((e) => setError(String(e)));
  };

  const call = (cmd: string, args: Record<string, unknown>) => {
    setError(null);
    invoke(cmd, args).catch((e) => setError(String(e)));
  };

  const saveRename = () => {
    if (editing) call("rename_source", { id: editing.id, label: editing.label });
    setEditing(null);
  };

  if (!state) return null;
  const profileName = (id: string) => state.profiles.find((p) => p.id === id)?.name ?? id;
  const selected = state.profiles.find((p) => p.id === profileId);
  const closedSources = state.saved.filter((s) => !s.running);

  const label = (s: SourceView) =>
    editing?.id === s.id ? (
      <input
        className="rename"
        autoFocus
        value={editing.label}
        onChange={(e) => setEditing({ id: s.id, label: e.target.value })}
        onBlur={saveRename}
        onKeyDown={(e) => {
          if (e.key === "Enter") saveRename();
          if (e.key === "Escape") setEditing(null);
        }}
      />
    ) : (
      <span className="label" title="Double-click to rename" onDoubleClick={() => setEditing({ id: s.id, label: s.label })}>
        {s.label}
      </span>
    );

  return (
    <main>
      <header>
        <h1>PiP Anywhere</h1>
        <p className="muted">Float any window on top of everything else.</p>
      </header>

      {error && (
        <div className="error" role="alert">
          <span>{error}</span>
          <button className="link" onClick={() => setError(null)} aria-label="Dismiss">×</button>
        </div>
      )}

      <section className="card">
        <h2>Pop out a window</h2>
        <div className="profiles" role="radiogroup" aria-label="Profile">
          {state.profiles.map((p) => (
            <button
              key={p.id}
              role="radio"
              aria-checked={p.id === profileId}
              className={p.id === profileId ? "chip active" : "chip"}
              onClick={() => setProfileId(p.id)}
            >
              {p.name}
            </button>
          ))}
        </div>
        {selected && (
          <p className="muted small">
            {selected.description} · {selected.fps} fps
          </p>
        )}
        <button className="primary" onClick={() => call("pop_out", { profileId })}>
          Choose window…
        </button>
      </section>

      <section className="card">
        <h2>Open</h2>
        {state.running.length === 0 ? (
          <p className="muted small">No floating windows.</p>
        ) : (
          <ul>
            {state.running.map((s) => (
              <li key={s.id} className="running">
                <div className="row">
                  <div>
                    {label(s)}
                    <span className="muted small">
                      {profileName(s.profile_id)} · {s.started ? "floating" : "waiting for selection…"}
                    </span>
                  </div>
                  <button onClick={() => call("close_pip", { id: s.id })}>Close</button>
                </div>
                {s.started && (
                  <label className="opacity">
                    <span className="muted small">Opacity</span>
                    <input
                      type="range"
                      min={20}
                      max={100}
                      step={5}
                      value={Math.round(s.opacity * 100)}
                      onChange={(e) => setOpacity(s.id, Number(e.target.value) / 100)}
                      aria-label={`Opacity of ${s.label}`}
                    />
                    <span className="small value">{Math.round(s.opacity * 100)}%</span>
                  </label>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      {closedSources.length > 0 && (
        <section className="card">
          <h2>Recent</h2>
          <ul>
            {closedSources.map((s) => (
              <li key={s.id}>
                <div>
                  {label(s)}
                  <span className="muted small">
                    {profileName(s.profile_id)} · {s.remembered ? "reopens without asking" : "asks for the window again"}
                  </span>
                </div>
                <div className="actions">
                  <button onClick={() => call("reopen", { id: s.id })}>Reopen</button>
                  <button className="link" onClick={() => call("forget", { id: s.id })} aria-label={`Forget ${s.label}`}>
                    Forget
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}

      <footer className="muted small">
        In a PiP: drag to move · drag edges to resize · Esc to close
        <button className="link" onClick={() => call("quit_app", {})}>Quit</button>
      </footer>
    </main>
  );
}

export default App;
