import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  AudioDevice,
  DiscoConfig,
  DiscoFrame,
  DiscoMode,
  DiscoStatus,
  UiConfig,
} from "./tauri";
import {
  IconMusic,
  IconPlay,
  IconSquare,
  IconSync,
} from "./icons";

/** Seconds of history shown in the colour timeline. */
const HISTORY_SECONDS = 6;
const FRAME_RATE = 30;
const HISTORY = HISTORY_SECONDS * FRAME_RATE;

const MODES: DiscoMode[] = ["beat", "spectrum", "pulse", "bands"];

/** Starting points per effect (tuned for the ~5 changes/s the L530 takes). */
const RECOMMENDED: Record<DiscoMode, Partial<DiscoConfig>> = {
  beat: { sensitivity: 0.5, gain: 1, min_brightness: 5, max_brightness: 100, release_ms: 250, hue_step: 60, saturation: 100, max_hz: 5, smoothing: 1, native: true },
  pulse: { sensitivity: 0.5, gain: 1, min_brightness: 3, max_brightness: 100, release_ms: 350, saturation: 100, max_hz: 5, smoothing: 1, native: true },
  spectrum: { sensitivity: 0.5, gain: 1, min_brightness: 15, max_brightness: 100, release_ms: 300, saturation: 100, max_hz: 5, smoothing: 0.6 },
  bands: { sensitivity: 0.5, gain: 1, min_brightness: 10, max_brightness: 100, release_ms: 250, saturation: 100, max_hz: 5, smoothing: 0.7 },
};


type Hist = { target: number[]; bulb: number[]; beat: boolean };

const rgb = (c: number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;

/** Size a canvas for the device pixel ratio and return its 2D context. */
function prep(cv: HTMLCanvasElement | null) {
  if (!cv) return null;
  const dpr = window.devicePixelRatio || 1;
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
  }
  const ctx = cv.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, w, h };
}

export function DiscoTab({
  t,
  cfg,
  setErr,
}: {
  t: (k: string) => string;
  cfg: UiConfig;
  setErr: (s: string | null) => void;
}) {
  const [conf, setConf] = useState<DiscoConfig | null>(null);
  const [on, setOn] = useState(false);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [status, setStatus] = useState<DiscoStatus>({ device: "", error: "" });
  const [bpm, setBpm] = useState(0);
  const [active, setActive] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [nativeLive, setNativeLive] = useState(false);
  const [nativeWaiting, setNativeWaiting] = useState(false);
  const [tempoConf, setTempoConf] = useState(0);

  const frameRef = useRef<DiscoFrame | null>(null);
  const histRef = useRef<Hist[]>([]);
  const peaksRef = useRef<number[]>([]);
  const beatGlowRef = useRef(0);
  const specRef = useRef<HTMLCanvasElement>(null);
  const timeRef = useRef<HTMLCanvasElement>(null);
  const bulbRef = useRef<HTMLDivElement>(null);
  const targetRef = useRef<HTMLDivElement>(null);
  const briRef = useRef<HTMLSpanElement>(null);
  const meterRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Load settings, devices and start the analysis preview while mounted.
  useEffect(() => {
    api
      .getDisco()
      .then((s) => {
        // Older saves may hold rates the bulb cannot reach (measured ~5/s).
        setConf({ ...s.config, max_hz: Math.min(s.config.max_hz, 5) });
        setOn(s.on);
        setStatus(s.status);
      })
      .catch((e) => setErr(e.message));
    api.listAudioOutputs().then(setDevices).catch(() => {});
    api.discoPreview(true).catch((e) => setErr(e.message));

    let lastUi = 0;
    const unFrame = listen<DiscoFrame>("disco-frame", (ev) => {
      const f = ev.payload;
      frameRef.current = f;
      const h = histRef.current;
      h.push({ target: f.target, bulb: f.bulb, beat: f.beat });
      if (h.length > HISTORY) h.splice(0, h.length - HISTORY);
      if (f.beat) beatGlowRef.current = 1;
      const now = performance.now();
      if (now - lastUi > 250) {
        lastUi = now;
        setBpm(Math.round(f.bpm));
        setActive(f.active);
        setNativeLive(f.native);
        setNativeWaiting(f.nativeWaiting);
        setTempoConf(f.tempoConf);
      }
    });
    const unStatus = listen<DiscoStatus>("disco-status", (ev) =>
      setStatus(ev.payload)
    );
    return () => {
      unFrame.then((u) => u());
      unStatus.then((u) => u());
      api.discoPreview(false).catch(() => {});
    };
  }, [setErr]);

  // Live-tune the running analysis whenever a setting changes.
  useEffect(() => {
    if (conf) api.discoTune(conf).catch(() => {});
  }, [conf]);

  // Render loop: draws the latest frame without re-rendering React.
  useEffect(() => {
    let raf = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const f = frameRef.current;
      beatGlowRef.current *= 0.88;

      // Spectrum.
      const sp = prep(specRef.current);
      if (sp) {
        const { ctx, w, h } = sp;
        ctx.clearRect(0, 0, w, h);
        const bands = f?.bands ?? [];
        const n = bands.length || 32;
        const peaks = peaksRef.current;
        const gap = 2;
        const bw = (w - gap * (n - 1)) / n;
        // Region guides: bass / mids / treble.
        ctx.fillStyle = "rgba(255,255,255,0.03)";
        ctx.fillRect(0, 0, w * 0.22, h);
        ctx.fillRect(w * 0.62, 0, w * 0.38, h);
        for (let i = 0; i < n; i++) {
          const v = bands[i] ?? 0;
          peaks[i] = Math.max(v, (peaks[i] ?? 0) - 0.012);
          const bh = Math.max(1, v * (h - 4));
          const x = i * (bw + gap);
          const hue = (i / (n - 1)) * 270;
          ctx.fillStyle = `hsl(${hue} 90% ${45 + v * 20}%)`;
          ctx.fillRect(x, h - bh, bw, bh);
          ctx.fillStyle = `hsla(${hue} 90% 75% / 0.9)`;
          ctx.fillRect(x, h - 2 - peaks[i] * (h - 4), bw, 2);
        }
        const g = beatGlowRef.current;
        if (g > 0.02) {
          ctx.strokeStyle = `rgba(255,255,255,${g * 0.8})`;
          ctx.lineWidth = 2;
          ctx.strokeRect(1, 1, w - 2, h - 2);
        }
      }

      // Timeline: target colour (top lane) vs simulated bulb (bottom lane).
      const tl = prep(timeRef.current);
      if (tl) {
        const { ctx, w, h } = tl;
        ctx.clearRect(0, 0, w, h);
        const hist = histRef.current;
        const sw = w / HISTORY;
        const lane = (h - 6) / 2;
        const off = HISTORY - hist.length;
        for (let i = 0; i < hist.length; i++) {
          const x = (off + i) * sw;
          ctx.fillStyle = rgb(hist[i].target);
          ctx.fillRect(x, 4, sw + 0.6, lane);
          ctx.fillStyle = rgb(hist[i].bulb);
          ctx.fillRect(x, 6 + lane, sw + 0.6, lane);
          if (hist[i].beat) {
            ctx.fillStyle = "#fff";
            ctx.fillRect(x, 0, Math.max(1.5, sw), 3);
          }
        }
      }

      // Bulb + meters.
      if (f) {
        const b = f.bulb;
        if (bulbRef.current) {
          const k = Math.max(0.15, f.bulbBri / 100);
          bulbRef.current.style.background = `radial-gradient(circle at 50% 40%, ${rgb(
            b.map((c) => Math.min(255, c + 40))
          )} 0%, ${rgb(b)} 55%, rgba(0,0,0,0.4) 100%)`;
          bulbRef.current.style.boxShadow = `0 0 ${10 + 40 * k}px ${4 + 14 * k}px rgba(${b[0]},${b[1]},${b[2]},${0.25 + 0.5 * k})`;
        }
        if (targetRef.current) targetRef.current.style.background = rgb(f.target);
        if (briRef.current) briRef.current.textContent = `${f.bulbBri}%`;
        [f.bass, f.mid, f.treble, f.level].forEach((v, i) => {
          const el = meterRefs.current[i];
          if (el) el.style.width = `${Math.round(v * 100)}%`;
        });
      }
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  if (!conf) return <section className="tab-pane amb-tab-pane" />;

  const nativeCapable = conf.mode === "beat" || conf.mode === "pulse";
  const nativeOn = nativeCapable && conf.native;

  const set = <K extends keyof DiscoConfig>(k: K, v: DiscoConfig[K]) => {
    setConf((c) => (c ? { ...c, [k]: v } : c));
    setDirty(true);
  };

  const apply = (enabled: boolean) => {
    setErr(null);
    setOn(enabled);
    setDirty(false);
    api.setDisco(enabled, conf).catch((e) => {
      setOn(false);
      setErr(e.message);
    });
  };

  /** Compact slider: label + value on one line, range below, hint on hover. */
  const slider = (
    label: string,
    badge: React.ReactNode,
    k: keyof DiscoConfig,
    min: number,
    max: number,
    step: number,
    hint: string,
    extraClass = ""
  ) => (
    <div className={`cs ${extraClass}`} title={t(hint)}>
      <div className="cs-top">
        <span className="cs-label">{label}</span>
        <span className="cs-val">{badge}</span>
      </div>
      <input
        type="range"
        className="slider cs-range"
        min={min}
        max={max}
        step={step}
        value={conf[k] as number}
        onChange={(e) => set(k, +e.target.value as never)}
      />
    </div>
  );

  return (
    <section className="tab-pane disco-tab-pane">
      {/* 1. Header: title, status pills, start/stop */}
      <div className={`disco-head ${on ? "active" : ""}`}>
        <div className="disco-head-main">
          <div className="disco-head-title">
            <IconMusic size={15} className="hero-icon" />
            <span>{t("disco.title")}</span>
          </div>
          <div className="disco-head-pills">
            <span className={`status-pill ${on ? "on" : "off"}`}>
              {on ? t("disco.running") : t("disco.stopped")}
            </span>
            {bpm > 0 && (
              <span className="disco-bpm" title={`${t("disco.tempo_conf")}: ${Math.round(tempoConf * 100)}%`}>
                {bpm} BPM
              </span>
            )}
            {nativeWaiting && (
              <span className="disco-bpm disco-wait-pill" title={t("disco.native.waiting.hint")}>
                {t("disco.native.waiting")}
              </span>
            )}
            {nativeLive && (
              <span className="disco-bpm disco-native-pill" title={t("disco.native.hint")}>
                {t("disco.native.on")}
              </span>
            )}
            {cfg.sync_all && (
              <span className="sync-pill" title={t("bulbs.sync_hint")}>
                <IconSync size={10} /> {t("bulbs.all_short")}
              </span>
            )}
          </div>
        </div>
        <button
          className={`btn-amb-hero disco-play ${on ? "active" : ""}`}
          onClick={() => apply(!on)}
          title={on ? t("disco.stop") : t("disco.start")}
        >
          {on ? <IconSquare size={13} /> : <IconPlay size={15} />}
        </button>
      </div>

      {/* 2. Live preview */}
      <div className="disco-preview" title={t("disco.preview")}>
        <div className={`disco-src ${status.error ? "err" : active ? "live" : ""}`}>
          {status.error
            ? status.error
            : status.device
            ? `${active ? "●" : "○"} ${status.device}`
            : t("disco.no_device")}
        </div>
        <canvas ref={specRef} className="disco-spectrum" />
        <div className="disco-row">
          <div className="disco-bulb-wrap" title={t("disco.bulb_hint")}>
            <div ref={bulbRef} className="disco-bulb" />
            <div className="disco-bulb-meta">
              <div ref={targetRef} className="disco-target" title={t("disco.target")} />
              <span ref={briRef}>0%</span>
            </div>
          </div>
          <div className="disco-meters">
            {[t("disco.bass"), t("disco.mid"), t("disco.treble"), t("disco.level")].map(
              (l, i) => (
                <div key={l} className="disco-meter">
                  <span>{l}</span>
                  <div className="disco-meter-track">
                    <div
                      ref={(el) => (meterRefs.current[i] = el)}
                      className={`disco-meter-fill m${i}`}
                    />
                  </div>
                </div>
              )
            )}
          </div>
        </div>
        <canvas
          ref={timeRef}
          className="disco-timeline"
          title={`${t("disco.timeline")} — ${t("disco.timeline_lanes")}`}
        />
      </div>

      {/* 3. Source + effect */}
      <div className="disco-card">
        <div className="disco-line">
          <span className="cs-label">{t("disco.device")}</span>
          <select
            className="disco-select"
            value={conf.device_id}
            onChange={(e) => set("device_id", e.target.value)}
          >
            <option value="">{t("disco.device_default")}</option>
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
                {d.isDefault ? " ★" : ""}
              </option>
            ))}
          </select>
        </div>
        <div className="mode-toggle-group disco-modes">
          {MODES.map((m) => (
            <button
              key={m}
              type="button"
              className={`mode-btn ${conf.mode === m ? "selected" : ""}`}
              onClick={() => set("mode", m)}
              title={t(`disco.mode.${m}.hint`)}
            >
              <span>{t(`disco.mode.${m}`)}</span>
            </button>
          ))}
        </div>
        <div className="disco-mode-hint">
          <span>{t(`disco.mode.${conf.mode}.hint`)}</span>
          <button
            type="button"
            className="disco-link"
            onClick={() => {
              setConf((c) => (c ? { ...c, ...RECOMMENDED[c.mode] } : c));
              setDirty(true);
            }}
          >
            {t("disco.recommended_short")}
          </button>
        </div>
      </div>

      {/* 4. Parameters (only the ones the effect uses) */}
      <div className="disco-card disco-params">
        <div className="disco-sec" title={t("disco.group.detect.hint")}>
          {t("disco.group.detect")}
        </div>
        {slider(t("disco.sensitivity"), `${Math.round(conf.sensitivity * 100)}%`, "sensitivity", 0, 1, 0.05, "disco.hint.sensitivity")}
        {slider(t("disco.gain"), `×${conf.gain.toFixed(1)}`, "gain", 0.5, 4, 0.1, "disco.hint.gain")}

        <div className="disco-sec">
          {t("disco.group.color")} · {t("disco.group.brightness")}
        </div>
        {conf.mode === "beat" &&
          slider(t("disco.hue_step"), `${conf.hue_step}°`, "hue_step", 10, 180, 5, "disco.hint.hue_step")}
        {conf.mode === "pulse" &&
          slider(t("disco.base_hue"), <><i className="cs-swatch" style={{ background: `hsl(${conf.base_hue} 95% 55%)` }} />{conf.base_hue}°</>, "base_hue", 0, 359, 1, "disco.hint.base_hue")}
        {slider(t("disco.saturation"), `${conf.saturation}%`, "saturation", 0, 100, 1, "disco.hint.saturation")}
        {slider(t("disco.min_bri"), `${conf.min_brightness}%`, "min_brightness", 0, 100, 1, "disco.hint.min_bri")}
        {slider(t("amb.maxbri"), `${conf.max_brightness}%`, "max_brightness", 1, 100, 1, "disco.hint.max_bri")}
        {!nativeOn &&
          slider(t("disco.release"), `${conf.release_ms} ms`, "release_ms", 50, 1500, 10, "disco.hint.release")}

        <div className="disco-sec" title={t("disco.group.send.hint")}>
          {t("disco.group.send")}
        </div>
        {nativeCapable && (
          <label className={`disco-native ${conf.native ? "on" : ""}`} title={t("disco.native.hint")}>
            <input
              type="checkbox"
              checked={conf.native}
              onChange={(e) => set("native", e.target.checked)}
            />
            <span>{t("disco.native")}</span>
          </label>
        )}
        {slider(t("amb.maxhz"), `${conf.max_hz} Hz`, "max_hz", 1, 5, 0.5, nativeOn ? "disco.native.fallback" : "disco.hint.max_hz", nativeOn ? "dim" : "")}
        {slider(t("disco.smoothing"), conf.smoothing >= 1 ? t("amb.mode.instant") : conf.smoothing.toFixed(2), "smoothing", 0.05, 1, 0.05, nativeOn ? "disco.native.fallback" : "disco.hint.smoothing", nativeOn ? "dim" : "")}
      </div>

      {/* 5. Save */}
      <button className="primary disco-save" onClick={() => apply(on)}>
        {dirty ? "● " : ""}
        {t("settings.save")}
        {on ? ` — ${t("disco.running")}` : ""}
      </button>
    </section>
  );
}
