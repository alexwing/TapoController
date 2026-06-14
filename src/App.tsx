import { useEffect, useState, useCallback, useMemo } from "react";
import {
  api,
  hexToRgb,
  UiConfig,
  DeviceInfo,
  Diagnosis,
  MonitorInfo,
} from "./tauri";
import { resolveLang, makeT, Lang } from "./i18n";

const PRESETS = [
  "#ff9329",
  "#cfe8ff",
  "#ff0000",
  "#00ff00",
  "#0033ff",
  "#7c3aed",
];

type Tab = "normal" | "streaming" | "settings";

export default function App() {
  const [cfg, setCfg] = useState<UiConfig | null>(null);
  const [tab, setTab] = useState<Tab>("normal");
  const [state, setState] = useState<DeviceInfo | null>(null);
  const [diag, setDiag] = useState<Diagnosis | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const lang: Lang = useMemo(
    () => resolveLang(cfg?.language ?? "system"),
    [cfg?.language]
  );
  const t = useMemo(() => makeT(lang), [lang]);

  const run = useCallback(async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setErr(null);
    try {
      await fn();
    } catch (e: any) {
      setErr(e.message ?? String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await api.getState();
      setState(s);
      setErr(null);
    } catch (e: any) {
      setState(null);
      setErr(e.message ?? String(e));
    }
  }, []);

  useEffect(() => {
    api.getConfig().then(setCfg).catch((e) => setErr(e.message));
    refresh();
    const id = setInterval(refresh, 5000);
    return () => clearInterval(id);
  }, [refresh]);

  const credsMissing = cfg && cfg.username.trim() === "";

  return (
    <div className="app">
      <header>
        <h1>TapoController</h1>
        <div className={`dot ${state ? "online" : "offline"}`}>
          {state
            ? `● ${state.nickname || state.model} · ${
                state.device_on ? t("status.online") : t("status.offline")
              }`
            : `● ${t("status.noconn")}`}
        </div>
      </header>

      <nav className="tabs">
        {(["normal", "streaming", "settings"] as Tab[]).map((tb) => (
          <button
            key={tb}
            className={tab === tb ? "active" : ""}
            onClick={() => setTab(tb)}
          >
            {t(`tab.${tb}`)}
          </button>
        ))}
      </nav>

      {credsMissing && <div className="warn">{t("creds.missing")}</div>}
      {err && <div className="error">{err}</div>}

      {tab === "normal" && (
        <NormalTab t={t} busy={busy} run={run} refresh={refresh} />
      )}
      {tab === "streaming" && cfg && (
        <StreamingTab t={t} cfg={cfg} setErr={setErr} />
      )}
      {tab === "settings" && cfg && (
        <SettingsTab
          t={t}
          cfg={cfg}
          diag={diag}
          onDiagnose={async () => {
            setErr(null);
            try {
              setDiag(await api.diagnose());
            } catch (e: any) {
              setErr(e.message);
            }
          }}
          onSave={async (c) => {
            await run(() => api.saveConfig(c));
            setCfg(c);
            refresh();
          }}
        />
      )}

      <footer>
        {t("footer.api")}:{" "}
        {cfg?.api_enabled
          ? `http://${cfg.api_bind}:${cfg.api_port}`
          : t("footer.api.off")}
      </footer>
    </div>
  );
}

function NormalTab({
  t,
  busy,
  run,
  refresh,
}: {
  t: (k: string) => string;
  busy: boolean;
  run: (fn: () => Promise<unknown>) => Promise<void>;
  refresh: () => void;
}) {
  const [bright, setBright] = useState(60);
  const [hue, setHue] = useState(30);
  const [sat, setSat] = useState(80);
  const [kelvin, setKelvin] = useState(4000);
  const [anim, setAnim] = useState(false);
  const [animSpeed, setAnimSpeed] = useState(40);

  useEffect(() => {
    api.getAnimation().then(setAnim).catch(() => {});
  }, []);

  const toggleAnim = (on: boolean, speed: number) => {
    setAnim(on);
    run(() => api.setAnimation(on, speed));
  };

  return (
    <section className="card">
      <div className="row power">
        <button
          className="primary"
          disabled={busy}
          onClick={() => run(() => api.setPower(true).then(refresh))}
        >
          {t("power.on")}
        </button>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => run(() => api.setPower(false).then(refresh))}
        >
          {t("power.off")}
        </button>
      </div>

      <label>
        {t("brightness")}: {bright}%
        <input
          type="range"
          min={1}
          max={100}
          value={bright}
          onChange={(e) => setBright(+e.target.value)}
          onMouseUp={() => run(() => api.setBrightness(bright))}
        />
      </label>
      <label>
        {t("hue")}: {hue}°
        <input
          type="range"
          min={0}
          max={360}
          value={hue}
          onChange={(e) => setHue(+e.target.value)}
          onMouseUp={() => run(() => api.setColor(hue, sat))}
        />
      </label>
      <label>
        {t("saturation")}: {sat}%
        <input
          type="range"
          min={0}
          max={100}
          value={sat}
          onChange={(e) => setSat(+e.target.value)}
          onMouseUp={() => run(() => api.setColor(hue, sat))}
        />
      </label>
      <label>
        {t("color.rgb")}
        <input
          type="color"
          defaultValue="#ff9329"
          onChange={(e) => {
            const [r, g, b] = hexToRgb(e.target.value);
            setAnim(false);
            run(() => api.setRgb(r, g, b));
          }}
        />
      </label>
      <label>
        {t("temperature")}: {kelvin} K
        <input
          type="range"
          min={2500}
          max={6500}
          step={100}
          value={kelvin}
          onChange={(e) => setKelvin(+e.target.value)}
          onMouseUp={() => run(() => api.setColorTemp(kelvin))}
        />
      </label>

      <label style={{ marginBottom: 6 }}>{t("whites")}</label>
      <div className="row whites">
        <button
          className="white-preset warm"
          disabled={busy}
          onClick={() => {
            setAnim(false);
            setKelvin(2700);
            setBright(100);
            run(() => api.setWhite(2700, 100));
          }}
        >
          ☀ {t("white.warm")}
        </button>
        <button
          className="white-preset cool"
          disabled={busy}
          onClick={() => {
            setAnim(false);
            setKelvin(6500);
            setBright(100);
            run(() => api.setWhite(6500, 100));
          }}
        >
          ❄ {t("white.cool")}
        </button>
      </div>

      <label style={{ marginBottom: 6 }}>{t("presets")}</label>
      <div className="presets">
        {PRESETS.map((p) => (
          <button
            key={p}
            style={{ background: p }}
            onClick={() => {
              const [r, g, b] = hexToRgb(p);
              setAnim(false);
              run(() => api.setRgb(r, g, b));
            }}
          />
        ))}
      </div>

      <div className="anim">
        <button
          className={anim ? "primary rainbow" : "secondary"}
          disabled={busy}
          onClick={() => toggleAnim(!anim, animSpeed)}
        >
          {anim ? t("anim.stop") : t("anim.start")}
        </button>
        <label>
          {t("anim.speed")}: {animSpeed}
          <input
            type="range"
            min={1}
            max={100}
            value={animSpeed}
            onChange={(e) => setAnimSpeed(+e.target.value)}
            onMouseUp={() => anim && toggleAnim(true, animSpeed)}
          />
        </label>
      </div>
    </section>
  );
}

function StreamingTab({
  t,
  cfg,
  setErr,
}: {
  t: (k: string) => string;
  cfg: UiConfig;
  setErr: (s: string | null) => void;
}) {
  const [mons, setMons] = useState<MonitorInfo[]>([]);
  const [on, setOn] = useState(false);
  const [monitor, setMonitor] = useState(cfg.stream_monitor);
  const [fps, setFps] = useState(cfg.stream_fps);
  const [mode, setMode] = useState(cfg.stream_mode);
  const [fade, setFade] = useState(cfg.stream_smoothing);
  const [maxHz, setMaxHz] = useState(cfg.stream_max_hz);
  const [maxBri, setMaxBri] = useState(cfg.stream_max_brightness);

  useEffect(() => {
    api.listMonitors().then(setMons).catch(() => {});
    api.getAmbilight().then(setOn).catch(() => {});
  }, []);

  const apply = (enabled: boolean) => {
    setErr(null);
    setOn(enabled);
    api
      .setAmbilight(enabled, monitor, fps, mode, fade, maxHz, maxBri)
      .catch((e) => setErr(e.message));
  };

  return (
    <section className="card">
      <h2>{t("amb.title")}</h2>
      <p className="hint">{t("amb.desc")}</p>

      <label className="inline">
        <input
          type="checkbox"
          checked={on}
          onChange={(e) => apply(e.target.checked)}
        />
        {t("amb.enable")}
      </label>

      <label>
        {t("amb.monitor")}
        <select
          value={monitor}
          onChange={(e) => setMonitor(+e.target.value)}
        >
          {mons.map((m) => (
            <option key={m.index} value={m.index}>
              #{m.index + 1} · {m.name} ({m.width}×{m.height})
              {m.primary ? " ★" : ""}
            </option>
          ))}
        </select>
      </label>

      <label>
        {t("amb.fps")}: {fps}
        <input
          type="range"
          min={1}
          max={30}
          value={fps}
          onChange={(e) => setFps(+e.target.value)}
        />
      </label>

      <label>
        {t("amb.mode")}
        <select value={mode} onChange={(e) => setMode(e.target.value)}>
          <option value="fade">{t("amb.mode.fade")}</option>
          <option value="instant">{t("amb.mode.instant")}</option>
        </select>
      </label>

      {mode === "fade" && (
        <label>
          {t("amb.fade")}: {fade.toFixed(2)}
          <input
            type="range"
            min={0.05}
            max={1}
            step={0.05}
            value={fade}
            onChange={(e) => setFade(+e.target.value)}
          />
        </label>
      )}

      <label>
        {t("amb.maxhz")}: {maxHz}
        <input
          type="range"
          min={1}
          max={20}
          step={0.5}
          value={maxHz}
          onChange={(e) => setMaxHz(+e.target.value)}
        />
      </label>

      <label>
        {t("amb.maxbri")}: {maxBri}%
        <input
          type="range"
          min={1}
          max={100}
          value={maxBri}
          onChange={(e) => setMaxBri(+e.target.value)}
        />
      </label>

      <button className="primary" onClick={() => apply(on)}>
        {on ? t("amb.running") : t("amb.stopped")} — {t("settings.save")}
      </button>
    </section>
  );
}

function SettingsTab({
  t,
  cfg,
  diag,
  onDiagnose,
  onSave,
}: {
  t: (k: string) => string;
  cfg: UiConfig;
  diag: Diagnosis | null;
  onDiagnose: () => void;
  onSave: (c: UiConfig) => void;
}) {
  const [l, setL] = useState<UiConfig>(cfg);
  const upd = (p: Partial<UiConfig>) => setL((x) => ({ ...x, ...p }));
  const [found, setFound] = useState<import("./tauri").DiscoveredBulb[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [detectMsg, setDetectMsg] = useState<string | null>(null);

  const detect = async () => {
    setDetecting(true);
    setDetectMsg(null);
    try {
      const list = await api.discoverBulbs();
      setFound(list);
      if (list.length === 0) setDetectMsg(t("settings.detect.none"));
      else if (list.length === 1) upd({ host: list[0].ip });
    } catch (e: any) {
      setDetectMsg(e.message ?? String(e));
    } finally {
      setDetecting(false);
    }
  };

  const label = (b: import("./tauri").DiscoveredBulb) =>
    [b.ip, b.nickname, b.model].filter(Boolean).join(" — ");

  return (
    <section className="card settings">
      <h2>{t("settings.connection")}</h2>
      <label>
        {t("settings.host")}
        <input
          list="discovered-bulbs"
          value={l.host}
          onChange={(e) => upd({ host: e.target.value })}
        />
        <datalist id="discovered-bulbs">
          {found.map((b) => (
            <option key={b.ip} value={b.ip}>
              {label(b)}
            </option>
          ))}
        </datalist>
      </label>
      <button className="ghost small" onClick={detect} disabled={detecting}>
        {detecting ? t("settings.detecting") : t("settings.detect")}
      </button>
      {detectMsg && <p className="hint">{detectMsg}</p>}
      {found.length > 0 && (
        <ul className="found">
          {found.map((b) => (
            <li key={b.ip}>
              <button className="linklike" onClick={() => upd({ host: b.ip })}>
                {label(b)}
                {b.klap ? "" : " (?)"}
              </button>
            </li>
          ))}
        </ul>
      )}
      <label>
        {t("settings.user")}
        <input
          value={l.username}
          onChange={(e) => upd({ username: e.target.value })}
        />
      </label>
      <label>
        {t("settings.pass")}
        <input
          type="password"
          value={l.password}
          onChange={(e) => upd({ password: e.target.value })}
        />
      </label>
      <label>
        {t("settings.protocol")}
        <select
          value={l.protocol ?? ""}
          onChange={(e) => upd({ protocol: e.target.value || null })}
        >
          <option value="">{t("settings.proto.auto")}</option>
          <option value="klap">KLAP</option>
          <option value="passthrough">Passthrough</option>
        </select>
      </label>

      <button className="ghost small" onClick={onDiagnose}>
        {t("diagnose")}
      </button>
      {diag && (
        <div className="warn">
          <b>{diag.verdict}</b> — {diag.host} (handshake1:{" "}
          {diag.handshake1_status ?? "—"})<br />
          {diag.message}
        </div>
      )}

      <h2>{t("settings.api")}</h2>
      <label className="inline">
        <input
          type="checkbox"
          checked={l.api_enabled}
          onChange={(e) => upd({ api_enabled: e.target.checked })}
        />
        {t("settings.api.enable")}
      </label>
      <label>
        {t("settings.api.bind")}
        <input
          value={l.api_bind}
          onChange={(e) => upd({ api_bind: e.target.value })}
        />
      </label>
      <label>
        {t("settings.api.port")}
        <input
          type="number"
          value={l.api_port}
          onChange={(e) => upd({ api_port: +e.target.value })}
        />
      </label>

      <h2>{t("settings.language")}</h2>
      <label>
        {t("settings.language")}
        <select
          value={l.language}
          onChange={(e) => upd({ language: e.target.value })}
        >
          <option value="system">{t("settings.lang.system")}</option>
          <option value="es">{t("settings.lang.es")}</option>
          <option value="en">{t("settings.lang.en")}</option>
        </select>
      </label>

      <button className="primary" onClick={() => onSave(l)}>
        {t("settings.save")}
      </button>
      <p className="hint">{t("settings.save.hint")}</p>
    </section>
  );
}
