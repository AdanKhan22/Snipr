import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, applyTheme } from "./api";

export default function Prompt() {
  const [label, setLabel] = useState("Value");
  const [value, setValue] = useState("");
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api.getState().then((s) => applyTheme(s.settings.theme));
    const u = listen<{ label: string; default: string }>("prompt-open", (e) => {
      setLabel(e.payload.label || "Value");
      setValue(e.payload.default || "");
      setTimeout(() => {
        input.current?.focus();
        input.current?.select();
      }, 30);
    });
    return () => {
      u.then((f) => f());
    };
  }, []);

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "Enter") api.submitPrompt(value);
    else if (e.key === "Escape") api.submitPrompt(null);
  }

  return (
    <div className="prompt" onKeyDown={onKey}>
      <label>{label}</label>
      <input ref={input} autoFocus value={value} onChange={(e) => setValue(e.target.value)} />
      <div className="row end">
        <button onClick={() => api.submitPrompt(null)}>Cancel</button>
        <button className="primary" onClick={() => api.submitPrompt(value)}>
          Insert
        </button>
      </div>
    </div>
  );
}
