import { invoke } from "@tauri-apps/api/core";

export interface BulbConfig {
  id: string;
  name: string;
  host: string;
  mac: string;
  username?: string | null;
  password?: string | null;
  protocol?: string | null;
  enabled: boolean;
}

export interface BulbStateSummary {
  id: string;
  name: string;
  host: string;
  mac: string;
  enabled: boolean;
  online: boolean;
  state: DeviceInfo | null;
  error: string | null;
}

export interface UiConfig {
  host: string;
  username: string;
  password: string;
  protocol: string | null;
  bulbs: BulbConfig[];
  active_bulb: string;
  sync_all: boolean;
  api_enabled: boolean;
  api_bind: string;
  api_port: number;
  stream_max_hz: number;
  stream_smoothing: number;
  stream_max_brightness: number;
  stream_mode: string;
  stream_monitor: number;
  stream_fps: number;
  language: string;
}

export interface MonitorInfo {
  index: number;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface DiscoveredBulb {
  ip: string;
  mac?: string | null;
  klap: boolean;
  nickname: string | null;
  model: string | null;
}

export interface DeviceInfo {
  device_id: string;
  model: string;
  fw_ver: string;
  mac: string;
  ip: string;
  device_on: boolean;
  brightness: number | null;
  hue: number | null;
  saturation: number | null;
  color_temp: number | null;
  nickname: string;
}

// Tauri command errors come back as { message: string }.
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e: any) {
    throw new Error(e?.message ?? String(e));
  }
}

export interface Diagnosis {
  host: string;
  reachable: boolean;
  handshake1_status: number | null;
  app_root_status: number | null;
  verdict: string;
  message: string;
}

export const api = {
  getConfig: () => call<UiConfig>("get_config"),
  saveConfig: (cfg: UiConfig) => call<void>("save_config", { new: cfg }),
  getAllBulbs: () => call<BulbStateSummary[]>("get_all_bulbs"),
  setActiveBulb: (target: string) =>
    call<void>("set_active_bulb", { target }),
  setSyncAll: (sync: boolean) => call<void>("set_sync_all", { sync }),
  relocateBulb: (bulbId: string) =>
    call<string>("relocate_bulb", { bulbId }),
  setBulbPower: (bulbId: string, on: boolean) =>
    call<void>("set_bulb_power", { bulbId, on }),
  setBulbBrightness: (bulbId: string, value: number) =>
    call<void>("set_bulb_brightness", { bulbId, value }),
  setBulbColor: (bulbId: string, hue: number, saturation: number) =>
    call<void>("set_bulb_color", { bulbId, hue, saturation }),
  setBulbRgb: (bulbId: string, r: number, g: number, b: number) =>
    call<void>("set_bulb_rgb", { bulbId, r, g, b }),
  setBulbColorTemp: (bulbId: string, kelvin: number) =>
    call<void>("set_bulb_color_temp", { bulbId, kelvin }),
  setBulbWhite: (bulbId: string, kelvin: number, brightness: number) =>
    call<void>("set_bulb_white", { bulbId, kelvin, brightness }),
  diagnose: (host?: string) =>
    call<Diagnosis>("run_diagnose", { host: host ?? null }),

  discoverBulbs: () => call<DiscoveredBulb[]>("discover_bulbs"),
  getState: () => call<DeviceInfo>("get_state"),
  setPower: (on: boolean) => call<void>("set_power", { on }),
  setBrightness: (value: number) => call<void>("set_brightness", { value }),
  setColor: (hue: number, saturation: number) =>
    call<void>("set_color", { hue, saturation }),
  setRgb: (r: number, g: number, b: number) =>
    call<void>("set_rgb", { r, g, b }),
  setColorTemp: (kelvin: number) => call<void>("set_color_temp", { kelvin }),
  setWhite: (kelvin: number, brightness: number) =>
    call<void>("set_white", { kelvin, brightness }),
  submitStreamColor: (r: number, g: number, b: number) =>
    call<void>("submit_stream_color", { r, g, b }),
  setAnimation: (on: boolean, speed?: number) =>
    call<void>("set_animation", { on, speed }),
  getAnimation: () => call<boolean>("get_animation"),
  listMonitors: () => call<MonitorInfo[]>("list_monitors"),
  getAmbilight: () => call<boolean>("get_ambilight"),
  setAmbilight: (
    on: boolean,
    monitor: number,
    fps: number,
    mode: string,
    smoothing: number,
    maxHz: number,
    maxBrightness: number
  ) =>
    call<void>("set_ambilight", {
      on,
      monitor,
      fps,
      mode,
      smoothing,
      maxHz,
      maxBrightness,
    }),
  openHelpWindow: () => call<void>("open_help_window"),
};

export function hexToRgb(hex: string): [number, number, number] {
  const v = hex.replace("#", "");
  return [
    parseInt(v.slice(0, 2), 16),
    parseInt(v.slice(2, 4), 16),
    parseInt(v.slice(4, 6), 16),
  ];
}
