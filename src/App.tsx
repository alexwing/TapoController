import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import {
  api,
  hexToRgb,
  UiConfig,
  Diagnosis,
  MonitorInfo,
  BulbStateSummary,
  BulbConfig,
  DiscoveredBulb,
} from "./tauri";
import { resolveLang, makeT, Lang } from "./i18n";
import {
  IconPower,
  IconSun,
  IconTemp,
  IconPalette,
  IconSliders,
  IconSparkles,
  IconBulb,
  IconSync,
  IconWarm,
  IconNeutral,
  IconCool,
  IconChevron,
  IconMonitor,
  IconSettings,
  IconPlay,
  IconSquare,
  IconZap,
  IconWave,
} from "./icons";
import { HelpWindow } from "./HelpWindow";
import { DiscoTab } from "./DiscoTab";
import { IconMusic } from "./icons";

const PRESETS = [
  "#ff3b30",
  "#ff9500",
  "#ffd60a",
  "#34c759",
  "#00c7be",
  "#007aff",
  "#7c3aed",
  "#ff2d55",
];

type Tab = "normal" | "streaming" | "disco" | "settings";

export default function App() {
  const [cfg, setCfg] = useState<UiConfig | null>(null);
  const [tab, setTab] = useState<Tab>("normal");
  const [bulbs, setBulbs] = useState<BulbStateSummary[]>([]);
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
      const bList = await api.getAllBulbs().catch(() => []);
      setBulbs(bList);
      setErr(null);
    } catch (e: any) {
      setErr(e.message ?? String(e));
    }
  }, []);

  useEffect(() => {
    api.getConfig().then(setCfg).catch((e) => setErr(e.message));
    refresh();
    const id = setInterval(refresh, 4000);
    return () => clearInterval(id);
  }, [refresh]);

  const credsMissing = cfg && cfg.username.trim() === "";

  const handleSelectBulb = async (target: string) => {
    await api.setActiveBulb(target);
    if (cfg) {
      setCfg({ ...cfg, active_bulb: target, sync_all: target === "all" });
    }
    await refresh();
  };

  const handleToggleSync = async (sync: boolean) => {
    await api.setSyncAll(sync);
    if (cfg) {
      setCfg({
        ...cfg,
        sync_all: sync,
        active_bulb: sync ? "all" : cfg.active_bulb,
      });
    }
    await refresh();
  };

  const [showHelpModal, setShowHelpModal] = useState(false);

  const handleOpenHelp = () => {
    api.openHelpWindow().catch(() => {
      setShowHelpModal(true);
    });
  };

  return (
    <div className="app">
      <div className="top-nav-bar">
        <nav className="tabs">
          {(["normal", "streaming", "disco", "settings"] as Tab[]).map((tb) => (
            <button
              key={tb}
              className={tab === tb ? "active" : ""}
              onClick={() => setTab(tb)}
            >
              {tb === "normal" && <IconBulb size={15} />}
              {tb === "streaming" && <IconMonitor size={15} />}
              {tb === "disco" && <IconMusic size={15} />}
              {tb === "settings" && <IconSettings size={15} />}
              <span>{t(`tab.${tb}`)}</span>
            </button>
          ))}
        </nav>
        <button
          className="btn-help-top"
          onClick={handleOpenHelp}
          title={t("help.btn")}
        >
          ?
        </button>
      </div>

      {credsMissing && <div className="warn">{t("creds.missing")}</div>}
      {err && <div className="error">{err}</div>}

      {tab === "normal" && (
        <NormalTab
          t={t}
          cfg={cfg}
          bulbs={bulbs}
          busy={busy}
          run={run}
          refresh={refresh}
          onSelectBulb={handleSelectBulb}
          onToggleSync={handleToggleSync}
        />
      )}
      {tab === "streaming" && cfg && (
        <StreamingTab t={t} cfg={cfg} setErr={setErr} />
      )}
      {tab === "disco" && cfg && (
        <DiscoTab t={t} cfg={cfg} setErr={setErr} />
      )}
      {tab === "settings" && cfg && (
        <SettingsTab
          t={t}
          cfg={cfg}
          diag={diag}
          onDiagnose={async (host?: string) => {
            setErr(null);
            try {
              setDiag(await api.diagnose(host));
            } catch (e: any) {
              setErr(e.message);
            }
          }}
          onSave={async (c) => {
            await run(() => api.saveConfig(c));
            setCfg(c);
            refresh();
          }}
          onRelocate={async (bulbId: string) => {
            try {
              const newIp = await api.relocateBulb(bulbId);
              refresh();
              return newIp;
            } catch (e: any) {
              setErr(e.message ?? String(e));
              return null;
            }
          }}
          onOpenHelp={handleOpenHelp}
        />
      )}

      {showHelpModal && (
        <div
          className="help-modal-overlay"
          onClick={() => setShowHelpModal(false)}
        >
          <div
            className="help-modal-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <HelpWindow
              initialLang={lang}
              onClose={() => setShowHelpModal(false)}
            />
          </div>
        </div>
      )}
    </div>
  );
}

function NormalTab({
  t,
  cfg,
  bulbs,
  busy,
  run,
  refresh,
  onSelectBulb,
  onToggleSync,
}: {
  t: (k: string) => string;
  cfg: UiConfig | null;
  bulbs: BulbStateSummary[];
  busy: boolean;
  run: (fn: () => Promise<unknown>) => Promise<void>;
  refresh: () => void;
  onSelectBulb: (target: string) => Promise<void>;
  onToggleSync: (sync: boolean) => Promise<void>;
}) {
  const [bright, setBright] = useState(60);
  const [hue, setHue] = useState(30);
  const [sat, setSat] = useState(80);
  const [kelvin, setKelvin] = useState(4000);
  const [anim, setAnim] = useState(false);
  const [animSpeed, setAnimSpeed] = useState(40);
  const [colorMode, setColorMode] = useState<"presets" | "manual">("presets");
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    }
    if (dropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [dropdownOpen]);

  useEffect(() => {
    api.getAnimation().then(setAnim).catch(() => {});
  }, []);

  const toggleAnim = (on: boolean, speed: number) => {
    setAnim(on);
    run(() => api.setAnimation(on, speed));
  };

  const isSync = Boolean(cfg?.sync_all || cfg?.active_bulb === "all");
  const activeBulbId = cfg?.active_bulb ?? "all";

  const selectedBulbIds: string[] = useMemo(() => {
    if (isSync) {
      return bulbs.map((b) => b.id);
    }
    if (activeBulbId.includes(",")) {
      return activeBulbId.split(",").map((s) => s.trim()).filter(Boolean);
    }
    return activeBulbId ? [activeBulbId] : (bulbs[0] ? [bulbs[0].id] : []);
  }, [isSync, activeBulbId, bulbs]);

  const allSelected = bulbs.length > 0 && selectedBulbIds.length === bulbs.length;
  const isPartiallySelected = selectedBulbIds.length > 0 && selectedBulbIds.length < bulbs.length;

  const handleToggleSelectAll = (checked: boolean) => {
    if (checked) {
      onToggleSync(true);
      onSelectBulb("all");
    } else {
      if (bulbs.length > 0) {
        onToggleSync(false);
        onSelectBulb(bulbs[0].id);
      }
    }
  };

  const handleToggleBulbCheckbox = (bulbId: string) => {
    let newSelected: string[];
    if (selectedBulbIds.includes(bulbId)) {
      newSelected = selectedBulbIds.filter((id) => id !== bulbId);
      if (newSelected.length === 0) {
        return; // at least 1 bulb stays selected
      }
    } else {
      newSelected = [...selectedBulbIds, bulbId];
    }

    if (newSelected.length === bulbs.length) {
      onToggleSync(true);
      onSelectBulb("all");
    } else if (newSelected.length === 1) {
      onToggleSync(false);
      onSelectBulb(newSelected[0]);
    } else {
      onToggleSync(false);
      onSelectBulb(newSelected.join(","));
    }
  };

  const activeBulb = isSync
    ? bulbs.find((b) => b.online)
    : bulbs.find((b) => selectedBulbIds.includes(b.id) && b.online) ?? bulbs[0];

  const isAnyOn = isSync
    ? bulbs.some((b) => b.online && (b.state?.device_on ?? false))
    : bulbs.some(
        (b) =>
          selectedBulbIds.includes(b.id) &&
          b.online &&
          (b.state?.device_on ?? false)
      );

  const heroTitle = isSync
    ? t("bulbs.all")
    : selectedBulbIds.length === 1
    ? bulbs.find((b) => b.id === selectedBulbIds[0])?.name || t("status.bulb")
    : `✦ ${selectedBulbIds.length} ${t("bulbs.selected")}`;

  // Sync controls with current bulb state when loaded
  useEffect(() => {
    const s = activeBulb?.state;
    if (s) {
      if (typeof s.brightness === "number") setBright(s.brightness);
      if (typeof s.hue === "number") setHue(s.hue);
      if (typeof s.saturation === "number") setSat(s.saturation);
      if (typeof s.color_temp === "number" && s.color_temp > 0) setKelvin(s.color_temp);
    }
  }, [activeBulbId, isSync, activeBulb?.state?.brightness, activeBulb?.state?.color_temp]);

  return (
    <section className="tab-pane normal-tab-pane">
      {/* Multi-bulb Selector Dropdown with Checkboxes */}
      {bulbs.length > 0 && (
        <div className="bulb-dropdown-container" ref={dropdownRef}>
          <button
            type="button"
            className="bulb-dropdown-trigger"
            onClick={() => setDropdownOpen(!dropdownOpen)}
          >
            <div className="trigger-left">
              {allSelected ? (
                <>
                  <IconSync size={14} className="sync-icon" />
                  <span>{t("bulbs.all")} ({bulbs.length})</span>
                </>
              ) : selectedBulbIds.length === 1 ? (
                (() => {
                  const b = bulbs.find((x) => x.id === selectedBulbIds[0]);
                  return (
                    <>
                      <span className={`dot ${b?.online ? "online" : "offline"}`}>●</span>
                      <span>{b ? (b.name || b.host) : t("bulbs.all")}</span>
                    </>
                  );
                })()
              ) : (
                <>
                  <IconBulb size={14} />
                  <span>✦ {selectedBulbIds.length} {t("bulbs.selected")}</span>
                </>
              )}
            </div>
            <div className="trigger-right">
              <IconChevron size={12} up={dropdownOpen} />
            </div>
          </button>

          {dropdownOpen && (
            <div className="bulb-dropdown-menu">
              <label className="bulb-dropdown-item bulb-dropdown-all">
                <input
                  type="checkbox"
                  checked={allSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = isPartiallySelected;
                  }}
                  onChange={(e) => handleToggleSelectAll(e.target.checked)}
                />
                <IconSync size={13} />
                <span className="bulb-item-title">{t("bulbs.all")}</span>
                <span className="bulb-item-count">({bulbs.length})</span>
              </label>
              <div className="dropdown-divider" />
              {bulbs.map((b) => {
                const checked = selectedBulbIds.includes(b.id);
                return (
                  <label
                    key={b.id}
                    className={`bulb-dropdown-item ${!b.online ? "offline" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => handleToggleBulbCheckbox(b.id)}
                    />
                    <span className={`dot ${b.online ? "online" : "offline"}`}>●</span>
                    <span className="bulb-item-title">{b.name || b.host}</span>
                    <span className="bulb-item-host">{b.host}</span>
                  </label>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Hero Power Card */}
      <div className={`hero-power ${isAnyOn ? "on" : "off"}`}>
        <div className="hero-power-left">
          <div className="hero-power-title">
            <IconBulb size={18} className="hero-icon" />
            <span className="hero-name">
              {heroTitle}
            </span>
          </div>
          <div className="hero-power-tags">
            <span className={`status-pill ${isAnyOn ? "on" : "off"}`}>
              {isAnyOn ? t("status.online") : t("status.offline")}
            </span>
            {isAnyOn && <span className="bri-pill">{bright}%</span>}
          </div>
        </div>
        <div className="hero-power-right">
          <button
            className={`btn-power-hero ${isAnyOn ? "active" : ""}`}
            disabled={busy}
            onClick={() =>
              run(() => api.setPower(!isAnyOn).then(refresh))
            }
            title={isAnyOn ? t("power.off") : t("power.on")}
          >
            <IconPower size={22} />
          </button>
        </div>
      </div>

      {/* Brightness Slider */}
      <div className="control-box">
        <div className="control-header">
          <div className="control-label">
            <IconSun size={15} />
            <span>{t("brightness")}</span>
          </div>
          <span className="control-badge">{bright}%</span>
        </div>
        <input
          type="range"
          className="slider"
          min={1}
          max={100}
          value={bright}
          onChange={(e) => setBright(+e.target.value)}
          onMouseUp={() => run(() => api.setBrightness(bright))}
        />
      </div>

      {/* Temperature & White Chips */}
      <div className="control-box">
        <div className="control-header">
          <div className="control-label">
            <IconTemp size={15} />
            <span>{t("temperature")}</span>
          </div>
          <span className="control-badge">{kelvin} K</span>
        </div>
        <input
          type="range"
          className="slider slider-temp"
          min={2500}
          max={6500}
          step={100}
          value={kelvin}
          onChange={(e) => setKelvin(+e.target.value)}
          onMouseUp={() => run(() => api.setColorTemp(kelvin))}
        />
        <div className="white-chips">
          <button
            className={`chip warm ${kelvin <= 3000 ? "selected" : ""}`}
            disabled={busy}
            onClick={() => {
              setAnim(false);
              setKelvin(2700);
              run(() => api.setWhite(2700, bright));
            }}
          >
            <IconWarm size={13} />
            <span>2700K {t("white.warm")}</span>
          </button>
          <button
            className={`chip neutral ${kelvin > 3000 && kelvin < 5500 ? "selected" : ""}`}
            disabled={busy}
            onClick={() => {
              setAnim(false);
              setKelvin(4000);
              run(() => api.setWhite(4000, bright));
            }}
          >
            <IconNeutral size={13} />
            <span>4000K {t("white.neutral")}</span>
          </button>
          <button
            className={`chip cool ${kelvin >= 5500 ? "selected" : ""}`}
            disabled={busy}
            onClick={() => {
              setAnim(false);
              setKelvin(6500);
              run(() => api.setWhite(6500, bright));
            }}
          >
            <IconCool size={13} />
            <span>6500K {t("white.cool")}</span>
          </button>
        </div>
      </div>

      {/* Colors & Manual Modes */}
      <div className="control-box">
        <div className="control-header">
          <div className="color-mode-tabs">
            <button
              type="button"
              className={`color-mode-tab ${colorMode === "presets" ? "active" : ""}`}
              onClick={() => setColorMode("presets")}
            >
              <IconPalette size={13} />
              <span>{t("presets")}</span>
            </button>
            <button
              type="button"
              className={`color-mode-tab ${colorMode === "manual" ? "active" : ""}`}
              onClick={() => setColorMode("manual")}
            >
              <IconSliders size={13} />
              <span>{t("manual")}</span>
            </button>
          </div>
        </div>

        {colorMode === "presets" ? (
          <div className="colors-row">
            {PRESETS.map((p) => (
              <button
                key={p}
                className="color-dot"
                style={{ backgroundColor: p }}
                onClick={() => {
                  const [r, g, b] = hexToRgb(p);
                  setAnim(false);
                  run(() => api.setRgb(r, g, b));
                }}
              />
            ))}
          </div>
        ) : (
          <div className="manual-color-row">
            {/* Custom Color Picker Dot */}
            <label className="color-dot picker-trigger" title={t("color.rgb")}>
              <input
                type="color"
                className="hidden-picker"
                defaultValue="#ff9329"
                onChange={(e) => {
                  const [r, g, b] = hexToRgb(e.target.value);
                  setAnim(false);
                  run(() => api.setRgb(r, g, b));
                }}
              />
            </label>

            {/* Inlined Fine-tune Sliders */}
            <div className="manual-slider-item">
              <div className="manual-slider-info">
                <span className="sub-label">{t("hue")}</span>
                <span className="control-badge-compact">{hue}°</span>
              </div>
              <input
                type="range"
                className="slider slider-hue"
                min={0}
                max={360}
                value={hue}
                onChange={(e) => setHue(+e.target.value)}
                onMouseUp={() => run(() => api.setColor(hue, sat))}
              />
            </div>

            <div className="manual-slider-item">
              <div className="manual-slider-info">
                <span className="sub-label">{t("saturation")}</span>
                <span className="control-badge-compact">{sat}%</span>
              </div>
              <input
                type="range"
                className="slider slider-sat"
                min={0}
                max={100}
                value={sat}
                onChange={(e) => setSat(+e.target.value)}
                onMouseUp={() => run(() => api.setColor(hue, sat))}
              />
            </div>
          </div>
        )}
      </div>

      {/* Rainbow Mode (Inline Compact Bar) */}
      <div className={`rainbow-bar ${anim ? "active" : ""}`}>
        <button
          className={`btn-rainbow-toggle ${anim ? "running" : ""}`}
          disabled={busy}
          onClick={() => toggleAnim(!anim, animSpeed)}
        >
          <IconSparkles size={14} />
          <span>{anim ? t("anim.stop") : t("anim.rainbow")}</span>
        </button>
        <div className="rainbow-speed">
          <span className="speed-label">{animSpeed}%</span>
          <input
            type="range"
            min={1}
            max={100}
            value={animSpeed}
            onChange={(e) => setAnimSpeed(+e.target.value)}
            onMouseUp={() => anim && toggleAnim(true, animSpeed)}
          />
        </div>
      </div>

      {/* Quick Individual Bulbs List (when more than 1 bulb) */}
      {bulbs.length > 1 && (
        <div className="quick-bulbs-compact">
          <div className="quick-header">
            <IconBulb size={14} />
            <span>{t("bulbs.quick_toggle")}</span>
          </div>
          <div className="quick-bulbs-grid">
            {bulbs.map((b) => {
              const isOn = b.online && (b.state?.device_on ?? false);
              return (
                <div
                  key={b.id}
                  className={`quick-chip ${isOn ? "on" : "off"}`}
                  onClick={() =>
                    run(() => api.setBulbPower(b.id, !isOn).then(refresh))
                  }
                >
                  <div className="quick-chip-left">
                    <span className={`pill-dot ${isOn ? "on" : "off"}`} />
                    <span className="quick-chip-name">{b.name}</span>
                  </div>
                  <button
                    className={`btn-quick-pwr ${isOn ? "on" : "off"}`}
                    disabled={busy || !b.online}
                    onClick={(e) => {
                      e.stopPropagation();
                      run(() => api.setBulbPower(b.id, !isOn).then(refresh));
                    }}
                    title={isOn ? t("power.off") : t("power.on")}
                  >
                    <IconPower size={13} />
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}
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

  const apply = (
    enabled: boolean,
    overrides?: Partial<{
      monitor: number;
      fps: number;
      mode: string;
      fade: number;
      maxHz: number;
      maxBri: number;
    }>
  ) => {
    setErr(null);
    setOn(enabled);
    const m = overrides?.monitor ?? monitor;
    const f = overrides?.fps ?? fps;
    const md = overrides?.mode ?? mode;
    const fd = overrides?.fade ?? fade;
    const hz = overrides?.maxHz ?? maxHz;
    const mb = overrides?.maxBri ?? maxBri;
    api
      .setAmbilight(enabled, m, f, md, fd, hz, mb)
      .catch((e) => setErr(e.message));
  };

  return (
    <section className="tab-pane amb-tab-pane">
      {/* 1. Hero Ambilight Card */}
      <div className={`hero-amb ${on ? "active" : ""}`}>
        <div className="hero-amb-left">
          <div className="hero-amb-title">
            <IconMonitor size={18} className="hero-icon" />
            <span>{t("amb.title")}</span>
          </div>
          <div className="hero-amb-tags">
            <span className={`status-pill ${on ? "on" : "off"}`}>
              {on ? t("amb.running") : t("amb.stopped")}
            </span>
            {cfg.sync_all && (
              <span className="sync-pill" title={t("bulbs.sync_hint")}>
                <IconSync size={11} /> {t("bulbs.all_short")}
              </span>
            )}
          </div>
        </div>
        <div className="hero-amb-right">
          <button
            className={`btn-amb-hero ${on ? "active" : ""}`}
            onClick={() => apply(!on)}
            title={on ? t("amb.stopped") : t("amb.running")}
          >
            {on ? <IconSquare size={16} /> : <IconPlay size={18} />}
          </button>
        </div>
      </div>

      {/* 2. Monitor & Mode Selection (2 columns) */}
      <div className="amb-grid-2">
        <div className="control-box">
          <div className="control-header">
            <div className="control-label">
              <IconMonitor size={14} />
              <span>{t("amb.monitor")}</span>
            </div>
          </div>
          <select
            className="amb-select"
            value={monitor}
            onChange={(e) => {
              const val = +e.target.value;
              setMonitor(val);
              if (on) apply(true, { monitor: val });
            }}
          >
            {mons.map((m) => (
              <option key={m.index} value={m.index}>
                #{m.index + 1} · {m.name} ({m.width}×{m.height}){m.primary ? " ★" : ""}
              </option>
            ))}
          </select>
        </div>

        <div className="control-box">
          <div className="control-header">
            <div className="control-label">
              <IconSliders size={14} />
              <span>{t("amb.mode")}</span>
            </div>
          </div>
          <div className="mode-toggle-group">
            <button
              type="button"
              className={`mode-btn ${mode === "fade" ? "selected" : ""}`}
              onClick={() => {
                setMode("fade");
                if (on) apply(true, { mode: "fade" });
              }}
            >
              <IconWave size={13} />
              <span>{t("amb.mode.fade")}</span>
            </button>
            <button
              type="button"
              className={`mode-btn ${mode === "instant" ? "selected" : ""}`}
              onClick={() => {
                setMode("instant");
                if (on) apply(true, { mode: "instant" });
              }}
            >
              <IconZap size={13} />
              <span>{t("amb.mode.instant")}</span>
            </button>
          </div>
        </div>
      </div>

      {/* 3. Performance Sliders (FPS & Max Hz side by side) */}
      <div className="amb-grid-2">
        <div className="control-box">
          <div className="control-header">
            <div className="control-label">
              <span>{t("amb.fps")}</span>
            </div>
            <span className="control-badge">{fps} fps</span>
          </div>
          <input
            type="range"
            className="slider"
            min={1}
            max={30}
            value={fps}
            onChange={(e) => setFps(+e.target.value)}
            onMouseUp={() => on && apply(true, { fps })}
          />
        </div>

        <div className="control-box">
          <div className="control-header">
            <div className="control-label">
              <span>{t("amb.maxhz")}</span>
            </div>
            <span className="control-badge">{maxHz} Hz</span>
          </div>
          <input
            type="range"
            className="slider"
            min={1}
            max={20}
            step={0.5}
            value={maxHz}
            onChange={(e) => setMaxHz(+e.target.value)}
            onMouseUp={() => on && apply(true, { maxHz })}
          />
        </div>
      </div>

      {/* 4. Brightness Cap & Smoothing */}
      <div className="amb-grid-2">
        <div className="control-box">
          <div className="control-header">
            <div className="control-label">
              <IconSun size={14} />
              <span>{t("amb.maxbri")}</span>
            </div>
            <span className="control-badge">{maxBri}%</span>
          </div>
          <input
            type="range"
            className="slider"
            min={1}
            max={100}
            value={maxBri}
            onChange={(e) => setMaxBri(+e.target.value)}
            onMouseUp={() => on && apply(true, { maxBri })}
          />
        </div>

        {mode === "fade" ? (
          <div className="control-box">
            <div className="control-header">
              <div className="control-label">
                <IconWave size={14} />
                <span>{t("amb.fade")}</span>
              </div>
              <span className="control-badge">{fade.toFixed(2)}</span>
            </div>
            <input
              type="range"
              className="slider"
              min={0.05}
              max={1}
              step={0.05}
              value={fade}
              onChange={(e) => setFade(+e.target.value)}
              onMouseUp={() => on && apply(true, { fade })}
            />
          </div>
        ) : (
          <div className="control-box mode-instant-box">
            <IconZap size={15} />
            <span>{t("amb.mode.instant")}</span>
          </div>
        )}
      </div>

      {/* 5. Save & Apply Button */}
      <button className="primary amb-save-btn" onClick={() => apply(on)}>
        {on ? t("amb.running") : t("amb.stopped")} — {t("settings.save")}
      </button>
    </section>
  );
}

type SettingsSubTab = "account" | "bulbs" | "api";

function SettingsTab({
  t,
  cfg,
  diag,
  onDiagnose,
  onSave,
  onRelocate,
  onOpenHelp,
}: {
  t: (k: string) => string;
  cfg: UiConfig;
  diag: Diagnosis | null;
  onDiagnose: (host?: string) => void;
  onSave: (c: UiConfig) => void;
  onRelocate: (bulbId: string) => Promise<string | null>;
  onOpenHelp: () => void;
}) {
  const [subTab, setSubTab] = useState<SettingsSubTab>("account");
  const [l, setL] = useState<UiConfig>(cfg);
  const upd = (p: Partial<UiConfig>) => setL((x) => ({ ...x, ...p }));
  const [found, setFound] = useState<DiscoveredBulb[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [detectMsg, setDetectMsg] = useState<string | null>(null);
  const [relocatingId, setRelocatingId] = useState<string | null>(null);
  const [relocateMsg, setRelocateMsg] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedFeedback, setSavedFeedback] = useState(false);

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave(l);
      setSavedFeedback(true);
      setTimeout(() => setSavedFeedback(false), 2000);
    } finally {
      setSaving(false);
    }
  };

  const detect = async () => {
    setDetecting(true);
    setDetectMsg(null);
    try {
      const list = await api.discoverBulbs();
      setFound(list);
      if (list.length === 0) {
        setDetectMsg(t("settings.detect.none"));
      }
    } catch (e: any) {
      setDetectMsg(e.message ?? String(e));
    } finally {
      setDetecting(false);
    }
  };

  const addDiscoveredBulb = (d: DiscoveredBulb) => {
    const exists = l.bulbs.some(
      (b) =>
        b.host === d.ip ||
        (d.mac &&
          b.mac &&
          b.mac.replace(/[:-]/g, "").toUpperCase() ===
            d.mac.replace(/[:-]/g, "").toUpperCase())
    );
    if (exists) return;

    const newBulb: BulbConfig = {
      id: `bulb-${Date.now()}`,
      name: d.nickname || d.model || `Bombilla ${l.bulbs.length + 1}`,
      host: d.ip,
      mac: d.mac || "",
      username: null,
      password: null,
      protocol: d.klap ? "klap" : null,
      enabled: true,
    };
    upd({ bulbs: [...l.bulbs, newBulb] });
  };

  const addManualBulb = () => {
    const newBulb: BulbConfig = {
      id: `bulb-${Date.now()}`,
      name: `Bombilla ${l.bulbs.length + 1}`,
      host: "192.168.1.100",
      mac: "",
      username: null,
      password: null,
      protocol: null,
      enabled: true,
    };
    upd({ bulbs: [...l.bulbs, newBulb] });
  };

  const updateBulb = (id: string, patch: Partial<BulbConfig>) => {
    upd({
      bulbs: l.bulbs.map((b) => (b.id === id ? { ...b, ...patch } : b)),
    });
  };

  const deleteBulb = (id: string) => {
    if (l.bulbs.length <= 1) return;
    upd({ bulbs: l.bulbs.filter((b) => b.id !== id) });
  };

  const handleRelocate = async (b: BulbConfig) => {
    if (!b.mac) return;
    setRelocatingId(b.id);
    setRelocateMsg(null);
    try {
      const newIp = await onRelocate(b.id);
      if (newIp) {
        updateBulb(b.id, { host: newIp });
        setRelocateMsg(`${t("bulbs.relocated_ok")} ${newIp}`);
      }
    } finally {
      setRelocatingId(null);
    }
  };

  return (
    <section className="tab-pane settings-tab-pane">
      {/* Settings Subtabs Header Bar with Save button on top right */}
      <div className="settings-header-bar">
        <div className="settings-subtabs">
          <button
            type="button"
            className={`settings-subtab-btn ${subTab === "account" ? "active" : ""}`}
            onClick={() => setSubTab("account")}
          >
            <IconSettings size={13} />
            <span>{t("settings.tab.account")}</span>
          </button>
          <button
            type="button"
            className={`settings-subtab-btn ${subTab === "bulbs" ? "active" : ""}`}
            onClick={() => setSubTab("bulbs")}
          >
            <IconBulb size={13} />
            <span>{t("settings.tab.bulbs")}</span>
          </button>
          <button
            type="button"
            className={`settings-subtab-btn ${subTab === "api" ? "active" : ""}`}
            onClick={() => setSubTab("api")}
          >
            <IconZap size={13} />
            <span>{t("settings.tab.api")}</span>
          </button>
        </div>

        <button
          type="button"
          className={`btn-save-top ${savedFeedback ? "saved" : ""}`}
          onClick={handleSave}
          disabled={saving}
          title={t("settings.save")}
        >
          {saving ? "…" : savedFeedback ? t("settings.saved") : t("settings.save_short")}
        </button>
      </div>

      {/* 1. Cuenta Subtab */}
      {subTab === "account" && (
        <div className="settings-subtab-body">
          <div className="settings-section">
            <div className="section-title">
              <IconSettings size={15} />
              <span>{t("settings.connection")}</span>
            </div>
            <div className="grid-2">
              <label className="compact-label">
                <span>{t("settings.user")}</span>
                <input
                  type="text"
                  value={l.username}
                  onChange={(e) => upd({ username: e.target.value })}
                  placeholder="user@example.com"
                />
              </label>
              <label className="compact-label">
                <span>{t("settings.pass")}</span>
                <input
                  type="password"
                  value={l.password}
                  onChange={(e) => upd({ password: e.target.value })}
                  placeholder="••••••••"
                />
              </label>
            </div>
            <p className="hint" style={{ marginTop: 8 }}>
              {t("creds.missing")}
            </p>
          </div>

          <div className="settings-section">
            <div className="grid-2">
              {/* Language selection */}
              <div className="control-box">
                <div className="control-header">
                  <span className="sub-title">{t("settings.language")}</span>
                </div>
                <select
                  value={l.language}
                  onChange={(e) => upd({ language: e.target.value })}
                  className="compact-select"
                >
                  <option value="system">{t("settings.lang.system")}</option>
                  <option value="es">{t("settings.lang.es")}</option>
                  <option value="en">{t("settings.lang.en")}</option>
                  <option value="de">{t("settings.lang.de")}</option>
                  <option value="fr">{t("settings.lang.fr")}</option>
                  <option value="pt">{t("settings.lang.pt")}</option>
                </select>
              </div>

              {/* Help Window Button */}
              <div className="control-box">
                <div className="control-header">
                  <span className="sub-title">{t("help.title")}</span>
                </div>
                <button
                  type="button"
                  className="secondary btn-open-help"
                  onClick={onOpenHelp}
                >
                  📖 {t("help.btn")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* 2. Bombillas Subtab */}
      {subTab === "bulbs" && (
        <div className="settings-subtab-body">
          <div className="settings-section">
            <div className="section-title-row">
              <div className="section-title">
                <IconBulb size={15} />
                <span>{t("bulbs.title")}</span>
              </div>
              <label className="inline sync-checkbox">
                <input
                  type="checkbox"
                  checked={l.sync_all}
                  onChange={(e) => upd({ sync_all: e.target.checked })}
                />
                <span>{t("bulbs.sync_label")}</span>
              </label>
            </div>

            <div className="bulb-tools-row" style={{ marginBottom: 12 }}>
              <button className="secondary btn-tool" onClick={addManualBulb}>
                {t("bulbs.add")}
              </button>
              <button
                className="secondary btn-tool"
                onClick={detect}
                disabled={detecting}
              >
                {detecting ? t("settings.detecting") : `🔍 ${t("settings.detect")}`}
              </button>
            </div>

            {detectMsg && <p className="hint">{detectMsg}</p>}

            {/* Discovered Bulbs list */}
            {found.length > 0 && (
              <div className="discovered-card">
                {found.map((d) => {
                  const alreadyAdded = l.bulbs.some(
                    (b) =>
                      b.host === d.ip ||
                      (d.mac &&
                        b.mac &&
                        b.mac.replace(/[:-]/g, "").toUpperCase() ===
                          d.mac.replace(/[:-]/g, "").toUpperCase())
                  );
                  return (
                    <div key={d.ip} className="discovered-item">
                      <div className="info">
                        <div className="name">
                          {d.nickname || d.model || "Tapo Bulb"}
                        </div>
                        <div className="meta">
                          {d.ip}
                          {d.mac ? ` · ${d.mac}` : ""}
                        </div>
                      </div>
                      {alreadyAdded ? (
                        <span className="badge-added">{t("bulbs.added")}</span>
                      ) : (
                        <button
                          className="primary btn-mini"
                          onClick={() => addDiscoveredBulb(d)}
                        >
                          {t("bulbs.detect_add")}
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {l.bulbs.map((b, idx) => (
              <div key={b.id} className="bulb-card-compact">
                <div className="bulb-card-header">
                  <span className="bulb-index-name">
                    #{idx + 1} · {b.name}
                  </span>
                  <div className="bulb-header-actions">
                    <label className="inline bulb-enabled-toggle">
                      <input
                        type="checkbox"
                        checked={b.enabled}
                        onChange={(e) =>
                          updateBulb(b.id, { enabled: e.target.checked })
                        }
                      />
                      <span>{b.enabled ? t("bulbs.enabled") : t("bulbs.disabled")}</span>
                    </label>
                    {l.bulbs.length > 1 && (
                      <button
                        className="danger-ghost-btn"
                        onClick={() => deleteBulb(b.id)}
                        title={t("bulbs.delete")}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                </div>

                <div className="grid-2">
                  <label className="compact-label">
                    <span>{t("bulbs.name")}</span>
                    <input
                      type="text"
                      value={b.name}
                      onChange={(e) => updateBulb(b.id, { name: e.target.value })}
                    />
                  </label>
                  <label className="compact-label">
                    <span>{t("bulbs.ip")}</span>
                    <input
                      type="text"
                      value={b.host}
                      onChange={(e) => updateBulb(b.id, { host: e.target.value })}
                    />
                  </label>
                </div>

                <div className="mac-action-row">
                  <label className="compact-label mac-field">
                    <span>{t("bulbs.mac")}</span>
                    <input
                      type="text"
                      value={b.mac}
                      placeholder="3C:6A:D2:7A:13:3E"
                      onChange={(e) => updateBulb(b.id, { mac: e.target.value })}
                    />
                  </label>
                  <div className="mac-buttons">
                    <button
                      className="secondary btn-mini"
                      disabled={!b.mac || relocatingId === b.id}
                      onClick={() => handleRelocate(b)}
                      title={t("bulbs.mac_hint")}
                    >
                      {relocatingId === b.id
                        ? t("bulbs.relocating")
                        : t("bulbs.relocate")}
                    </button>
                    <button
                      className="ghost btn-mini"
                      onClick={() => onDiagnose(b.host)}
                    >
                      {t("diagnose")}
                    </button>
                  </div>
                </div>
              </div>
            ))}

            {relocateMsg && <div className="warn">{relocateMsg}</div>}

            {diag && (
              <div className="warn" style={{ marginTop: 10 }}>
                <b>{diag.verdict}</b> — {diag.host} (handshake1:{" "}
                {diag.handshake1_status ?? "—"})<br />
                {diag.message}
              </div>
            )}
          </div>
        </div>
      )}

      {/* 3. API Subtab */}
      {subTab === "api" && (
        <div className="settings-subtab-body">
          <div className="settings-section">
            <div className="api-config-box">
              <div className="api-config-header">
                <label className="inline" style={{ margin: 0 }}>
                  <input
                    type="checkbox"
                    checked={l.api_enabled}
                    onChange={(e) => upd({ api_enabled: e.target.checked })}
                  />
                  <span>{t("settings.api.enable")}</span>
                </label>
                <div className="api-port-row">
                  <span className="api-port-lbl">Port:</span>
                  <input
                    type="number"
                    className="api-port-input"
                    value={l.api_port}
                    onChange={(e) => upd({ api_port: +e.target.value })}
                  />
                </div>
              </div>
              <div className="api-url-info">
                <span className="api-label">{t("footer.api")}:</span>
                <code>
                  {l.api_enabled
                    ? `http://${l.api_bind}:${l.api_port}`
                    : t("footer.api.off")}
                </code>
              </div>
            </div>

            <div className="api-hints-box" style={{ marginTop: 12 }}>
              <div className="sub-title" style={{ marginBottom: 6 }}>Endpoints:</div>
              <div className="api-endpoint-list">
                <div><code>GET /state</code> — {t("status.bulb")}</div>
                <div><code>GET /bulbs</code> — {t("bulbs.title")}</div>
                <div><code>POST /power</code> — {`{"on": true}`}</div>
                <div><code>POST /brightness</code> — {`{"brightness": 80}`}</div>
                <div><code>POST /color_temp</code> — {`{"color_temp": 4000}`}</div>
                <div><code>POST /rgb</code> — {`{"r": 255, "g": 128, "b": 0}`}</div>
                <div><code>POST /sync</code> — {`{"sync_all": true}`}</div>
              </div>
            </div>

            <p className="hint text-center" style={{ margin: "14px 0 0" }}>
              {t("settings.save.hint")}
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
