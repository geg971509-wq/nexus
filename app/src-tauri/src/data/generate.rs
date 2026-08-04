//! Pure generate: profile/settings → sing-box JSON (MVP minimal).
use super::store::{Profile, Store};
use serde_json::{json, Value};

pub struct GenerateInput<'a> {
    pub profile: &'a Profile,
    pub system_proxy_port: u16,
    pub tun: bool,
}

/// Pure function — no UI/socket.
pub fn generate_config(input: &GenerateInput<'_>) -> Value {
    let mut outbound = input.profile.outbound.clone();
    if outbound.get("tag").is_none() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert("tag".into(), json!("proxy"));
        }
    }
    let outbounds = vec![outbound, json!({"type":"direct","tag":"direct"})];

    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": "127.0.0.1",
        "listen_port": input.system_proxy_port
    })];

    if input.tun {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "inet4_address": "172.19.0.1/30",
            "auto_route": true,
            "strict_route": true,
            "stack": "system"
        }));
    }

    json!({
        "log": {"level": "info"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "final": "proxy",
            "auto_detect_interface": true
        }
    })
}

pub fn generate_for_store(store: &Store, port: u16) -> Result<Value, String> {
    let id = store
        .selected_profile_id
        .as_ref()
        .ok_or("no selected profile")?;
    let profile = store
        .profiles
        .iter()
        .find(|p| &p.id == id)
        .ok_or("selected profile missing")?;
    Ok(generate_config(&GenerateInput {
        profile,
        system_proxy_port: port,
        tun: store.tun,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::store::Profile;

    #[test]
    fn generate_has_mixed_and_proxy() {
        let p = Profile {
            id: "1".into(),
            name: "t".into(),
            group_id: "g".into(),
            outbound: json!({"type":"socks","tag":"proxy","server":"127.0.0.1","server_port":1080}),
        };
        let v = generate_config(&GenerateInput {
            profile: &p,
            system_proxy_port: 2080,
            tun: false,
        });
        assert_eq!(v["inbounds"][0]["listen_port"], 2080);
        assert_eq!(v["outbounds"][0]["type"], "socks");
    }
}
