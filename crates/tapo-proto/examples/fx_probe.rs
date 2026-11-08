//! Bench/probe for fast light changes on a real bulb (reads ./tapo-config.toml,
//! first enabled bulb).
//!
//!   cargo run -p tapo-proto --example fx_probe -- rules
//!   cargo run -p tapo-proto --example fx_probe -- latency 30
//!   cargo run -p tapo-proto --example fx_probe -- fx <change_time_ms> <change_mode> <secs> [pulse|rainbow]
//!   cargo run -p tapo-proto --example fx_probe -- raw <method> [json]
//!   cargo run -p tapo-proto --example fx_probe -- clear
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tapo_proto::{TapoConfig, TapoDevice};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = TapoConfig::load("tapo-config.toml")?;
    let bulb = cfg
        .bulbs
        .iter()
        .find(|b| b.enabled)
        .ok_or_else(|| anyhow::anyhow!("no bulb"))?;
    let (u, p) = bulb.effective_credentials(&cfg.device.username, &cfg.device.password);
    let t = Instant::now();
    let dev = TapoDevice::connect(&bulb.host, u, p).await?;
    println!("connected to {} in {:?}", bulb.host, t.elapsed());

    match args.first().map(String::as_str) {
        Some("rules") => {
            for m in ["get_dynamic_light_effect_rules", "get_device_info"] {
                match dev.call_raw(m, None).await {
                    Ok(v) => println!("== {m}\n{}", filter(&v)),
                    Err(e) => println!("== {m}: ERR {e}"),
                }
            }
        }
        Some("latency") => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
            let mut times = Vec::new();
            for i in 0..n {
                let (h, b) = if i % 2 == 0 { (0, 100) } else { (240, 20) };
                let t = Instant::now();
                let r = dev.set_color_brightness(h, 100, b).await;
                let dt = t.elapsed();
                if let Err(e) = r {
                    println!("#{i} ERR {e}");
                }
                times.push(dt.as_secs_f64() * 1000.0);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let sum: f64 = times.iter().sum();
            println!(
                "n={n} min={:.0}ms median={:.0}ms p90={:.0}ms max={:.0}ms -> {:.1} cmds/s back-to-back",
                times[0],
                times[n / 2],
                times[n * 9 / 10],
                times[n - 1],
                n as f64 / (sum / 1000.0)
            );
        }
        Some("fx") => {
            let ct: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(400);
            let mode = args.get(2).cloned().unwrap_or_else(|| "bln".into());
            let secs: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(6);
            let kind = args.get(4).map(String::as_str).unwrap_or("pulse");
            // stops: [brightness, hue, saturation, colortemp]
            let stops: Vec<[u32; 4]> = match kind {
                "rainbow" => (0..8).map(|i| [100, i * 45, 100, 0]).collect(),
                // magenta bright/dim alternating = brightness pulse
                _ => (0..8)
                    .map(|i| [if i % 2 == 0 { 100 } else { 5 }, 300, 100, 0])
                    .collect(),
            };
            let rule = json!({
                "id": "L1",
                "scene_name": "cHJvYmU=",
                "change_mode": mode,
                "change_time": ct,
                "color_status_list": stops,
            });
            let t = Instant::now();
            let r1 = dev.call_raw("edit_dynamic_light_effect_rule", Some(rule)).await;
            let t1 = t.elapsed();
            let r2 = dev
                .call_raw(
                    "set_dynamic_light_effect_rule_enable",
                    Some(json!({"enable": true, "id": "L1"})),
                )
                .await;
            println!(
                "edit: {:?} ({:?}) enable: {:?} ({:?})",
                r1.map(|v| filter(&v)),
                t1,
                r2.map(|v| filter(&v)),
                t.elapsed()
            );
            if let Ok(v) = dev.call_raw("get_dynamic_light_effect_rules", Some(json!({"start_index": 0}))).await {
                let l1 = v["rule_list"]
                    .as_array()
                    .and_then(|l| l.iter().find(|r| r["id"] == "L1"))
                    .cloned()
                    .unwrap_or(Value::Null);
                println!("stored L1 = {l1}");
            }
            // Sample what the bulb reports while the effect runs.
            let t0 = Instant::now();
            let mut last = String::new();
            while t0.elapsed() < Duration::from_secs(secs) {
                if let Ok(v) = dev.call_raw("get_device_info", None).await {
                    let cur = format!(
                        "on={} fx={} bri={} hue={} sat={}",
                        v["device_on"], v["dynamic_light_effect_enable"], v["brightness"], v["hue"], v["saturation"]
                    );
                    if cur != last {
                        println!("{:6}ms {cur}", t0.elapsed().as_millis());
                        last = cur;
                    }
                }
            }
            dev.clear_effect().await?;
            println!("cleared");
        }
        Some("readlat") => {
            let mut t2 = Vec::new();
            for _ in 0..15 {
                let t = Instant::now();
                dev.call_raw("get_device_info", None).await?;
                t2.push(t.elapsed().as_millis());
            }
            t2.sort();
            println!("get_device_info ms: min={} median={} max={}", t2[0], t2[7], t2[14]);
            let mut t3 = Vec::new();
            for i in 0..15 {
                let t = Instant::now();
                dev.call_raw("set_device_info", Some(json!({"brightness": if i % 2 == 0 { 60 } else { 40 }}))).await?;
                t3.push(t.elapsed().as_millis());
            }
            t3.sort();
            println!("set brightness only ms: min={} median={} max={}", t3[0], t3[7], t3[14]);
        }
        Some("parallel") => {
            let k: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
            let rounds = 10;
            let t = Instant::now();
            let mut errs = 0;
            for r in 0..rounds {
                let futs = (0..k).map(|j| {
                    let h = ((r * k + j) * 50 % 360) as u16;
                    dev.set_color_brightness(h, 100, 80)
                });
                for res in futures_util::future::join_all(futs).await {
                    if let Err(e) = res {
                        errs += 1;
                        println!("ERR {e}");
                    }
                }
            }
            let el = t.elapsed().as_secs_f64();
            println!("k={k}: {} cmds in {:.2}s -> {:.1} cmds/s, errors={errs}", rounds * k, el, (rounds * k) as f64 / el);
        }
        Some("accept") => {
            // Which change_mode / change_time values does the firmware take?
            let stops: Vec<[u32; 4]> = (0..8).map(|i| [100, i * 45, 100, 0]).collect();
            let modes = ["bln", "ln", "direct", "jump", "flash", "breath", "blink", "strobe", "gradual", "bl"];
            for m in modes {
                let r = dev.call_raw("edit_dynamic_light_effect_rule", Some(json!({
                    "id": "L1", "scene_name": "cHJvYmU=", "change_mode": m,
                    "change_time": 1000, "color_status_list": stops,
                }))).await;
                println!("mode {m:8} -> {}", match r { Ok(_) => "OK".to_string(), Err(e) => e.to_string() });
            }
            for ct in [50u32, 100, 150, 200, 250, 300, 400] {
                let r = dev.call_raw("edit_dynamic_light_effect_rule", Some(json!({
                    "id": "L1", "scene_name": "cHJvYmU=", "change_mode": "bln",
                    "change_time": ct, "color_status_list": stops,
                }))).await;
                let stored = dev.call_raw("get_dynamic_light_effect_rules", Some(json!({"start_index": 0}))).await
                    .ok().and_then(|v| v["rule_list"][0]["change_time"].as_u64());
                println!("change_time {ct:4} -> {} stored={stored:?}", match r { Ok(_) => "OK".to_string(), Err(e) => e.to_string() });
            }
            for n in [2usize, 4, 16] {
                let st: Vec<[u32; 4]> = (0..n).map(|i| [100, (i * 40 % 360) as u32, 100, 0]).collect();
                let r = dev.call_raw("edit_dynamic_light_effect_rule", Some(json!({
                    "id": "L1", "scene_name": "cHJvYmU=", "change_mode": "bln",
                    "change_time": 1000, "color_status_list": st,
                }))).await;
                println!("{n} stops -> {}", match r { Ok(_) => "OK".to_string(), Err(e) => e.to_string() });
            }
            // restore the app's rainbow rule
            dev.call_raw("edit_dynamic_light_effect_rule", Some(json!({
                "id": "L1", "scene_name": "Y29sb3Jlcw==", "change_mode": "bln",
                "change_time": 2255, "color_status_list": stops,
            }))).await?;
            println!("restored L1");
        }
        Some("raw") => {
            let m = args.get(1).expect("method");
            let p = args.get(2).map(|s| serde_json::from_str(s)).transpose()?;
            println!("{}", filter(&dev.call_raw(m, p).await?));
        }
        Some("clear") => dev.clear_effect().await?,
        _ => println!("usage: rules | latency N | fx ct mode secs [pulse|rainbow] | raw m [json] | clear"),
    }
    Ok(())
}

/// Hide identifying fields before printing.
fn filter(v: &Value) -> String {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        for k in ["ssid", "mac", "ip", "device_id", "hw_id", "fw_id", "oem_id", "nickname", "latitude", "longitude", "region", "time_diff"] {
            o.remove(k);
        }
    }
    serde_json::to_string_pretty(&v).unwrap()
}
