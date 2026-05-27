import { invoke } from "@tauri-apps/api/core";

export interface UiConfig {
  host: string;
  username: string;
  password: string;
  protocol: string | null;
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
  diagnose: () => call<Diagnosis>("run_diagnose"),
  discoverBulbs: () => call<DiscoveredBulb[]>("discover_bulbs"),
  getState: () => call<DeviceInfo>("get_state"),
  setPower: (on: boolean) => call<void>("set_power", { on }),
  setBrightness: (value: number) => call<void>("set_brightness", { value }),
  setColor: (hue: number, saturation: number) =>
    call<void>("set_color", { hue, saturation }),
  setRgb: (r: number, g: number, b: number) =>
    call<void>("set_rgb", { r, g, b }),
  setColorTemp: (kelvin: number) => call<void>("set_color_temp", { kelvin }),
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
};

export function hexToRgb(hex: string): [number, number, number] {
  const v = hex.replace("#", "");
  return [
    parseInt(v.slice(0, 2), 16),
    parseInt(v.slice(2, 4), 16),
    parseInt(v.slice(4, 6), 16),
  ];
}
