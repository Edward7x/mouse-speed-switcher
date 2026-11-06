use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameters {
    pub speed: i32,
    pub acceleration: [i32; 3],
}
impl Parameters {
    pub fn precision(self) -> bool {
        self.acceleration[2] != 0
    }
    pub fn with_precision(mut self, enabled: bool) -> Self {
        if enabled && !self.precision() {
            self.acceleration = [6, 10, 1];
        }
        if !enabled {
            self.acceleration[2] = 0;
        }
        self
    }
    pub fn valid(self) -> bool {
        (1..=20).contains(&self.speed)
            && self.acceleration[0] >= 0
            && self.acceleration[1] >= 0
            && (0..=2).contains(&self.acceleration[2])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub key: String,
    pub name: String,
    pub instance: String,
    pub interface: String,
    pub container: Option<String>,
    pub serial: Option<String>,
}
impl Identity {
    pub fn stable_key(container: Option<&str>, instance: &str, interface: &str) -> String {
        if let Some(id) = container.filter(|s| !s.is_empty()) {
            format!("container:{}", id.to_lowercase())
        } else if !instance.is_empty() {
            format!("instance:{}", instance.to_lowercase())
        } else {
            format!("interface:{}", interface.to_lowercase())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub alias: String,
    pub identity: Identity,
    pub parameters: Parameters,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub profiles: BTreeMap<String, Profile>,
    pub bindings: BTreeMap<String, String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: BTreeMap::new(),
            bindings: BTreeMap::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("不支持的配置版本".into());
        }
        if self
            .profiles
            .values()
            .any(|p| !p.parameters.valid() || p.alias.trim().is_empty())
        {
            return Err("配置中有无效的速度、名称或加速参数".into());
        }
        if self
            .bindings
            .values()
            .any(|id| !self.profiles.contains_key(id))
        {
            return Err("设备关联指向不存在的配置".into());
        }
        Ok(())
    }
    pub fn discover(&mut self, device: &Identity, initial: Parameters) -> (String, bool) {
        if let Some(id) = self.bindings.get(&device.key) {
            return (id.clone(), false);
        }
        let id = device.key.clone();
        self.profiles.entry(id.clone()).or_insert_with(|| Profile {
            alias: device.name.clone(),
            identity: device.clone(),
            parameters: initial,
        });
        self.bindings.insert(device.key.clone(), id.clone());
        (id, true)
    }
}

pub trait MouseSystem {
    fn read(&self) -> Result<Parameters, String>;
    fn write(&mut self, value: Parameters) -> Result<(), String>;
}
pub fn sole_device(devices: &BTreeMap<usize, Identity>) -> Option<usize> {
    let first = devices.iter().next()?;
    devices
        .values()
        .all(|d| d.key == first.1.key)
        .then_some(*first.0)
}
pub struct Switcher<S: MouseSystem> {
    pub system: S,
    pub initial: Parameters,
    pub last: Option<Parameters>,
    pub active: Option<String>,
    pub paused: bool,
}
impl<S: MouseSystem> Switcher<S> {
    pub fn new(system: S) -> Result<Self, String> {
        let initial = system.read()?;
        Ok(Self {
            system,
            initial,
            last: None,
            active: None,
            paused: false,
        })
    }
    pub fn activate(
        &mut self,
        id: &str,
        parameters: Parameters,
        force: bool,
    ) -> Result<(), String> {
        if self.paused && !force {
            return Ok(());
        }
        if self.last != Some(parameters) || force {
            self.system.write(parameters)?;
            self.last = Some(parameters);
        }
        if self.active.as_deref() != Some(id) {
            self.active = Some(id.to_owned());
        }
        Ok(())
    }
    pub fn restore(&mut self) -> Result<(), String> {
        if let Some(last) = self.last
            && self.system.read()? == last
        {
            self.system.write(self.initial)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEFAULT: Parameters = Parameters {
        speed: 10,
        acceleration: [6, 10, 1],
    };
    struct Fake {
        value: Parameters,
        writes: usize,
        fail: bool,
    }
    impl MouseSystem for Fake {
        fn read(&self) -> Result<Parameters, String> {
            Ok(self.value)
        }
        fn write(&mut self, v: Parameters) -> Result<(), String> {
            if self.fail {
                return Err("failure".into());
            }
            self.value = v;
            self.writes += 1;
            Ok(())
        }
    }
    fn engine() -> Switcher<Fake> {
        Switcher::new(Fake {
            value: DEFAULT,
            writes: 0,
            fail: false,
        })
        .unwrap()
    }
    #[test]
    fn deduplicates_high_frequency_input() {
        let mut s = engine();
        for _ in 0..8000 {
            s.activate("a", DEFAULT, false).unwrap();
        }
        s.activate("b", DEFAULT, false).unwrap();
        assert_eq!(s.system.writes, 1);
        assert_eq!(s.active.as_deref(), Some("b"));
    }
    #[test]
    fn failure_keeps_last_success() {
        let mut s = engine();
        s.activate("a", DEFAULT, false).unwrap();
        s.system.fail = true;
        assert!(
            s.activate(
                "b",
                Parameters {
                    speed: 2,
                    ..DEFAULT
                },
                false
            )
            .is_err()
        );
        assert_eq!(s.active.as_deref(), Some("a"));
        assert_eq!(s.last, Some(DEFAULT));
    }
    #[test]
    fn pause_and_manual_apply() {
        let mut s = engine();
        s.paused = true;
        s.activate("a", DEFAULT, false).unwrap();
        assert!(s.active.is_none());
        s.activate("a", DEFAULT, true).unwrap();
        assert_eq!(s.system.writes, 1);
    }
    #[test]
    fn restore_respects_external_changes() {
        let mut s = engine();
        s.activate(
            "a",
            Parameters {
                speed: 3,
                ..DEFAULT
            },
            false,
        )
        .unwrap();
        s.restore().unwrap();
        assert_eq!(s.system.value, DEFAULT);
        s.activate(
            "a",
            Parameters {
                speed: 4,
                ..DEFAULT
            },
            true,
        )
        .unwrap();
        s.system.value.speed = 9;
        s.restore().unwrap();
        assert_eq!(s.system.value.speed, 9);
    }
    #[test]
    fn stable_identity_is_not_vid_pid_only() {
        assert_ne!(
            Identity::stable_key(None, "HID\\VID_1234\\A", ""),
            Identity::stable_key(None, "HID\\VID_1234\\B", "")
        );
        assert_eq!(
            Identity::stable_key(Some("ABC"), "port1", ""),
            Identity::stable_key(Some("abc"), "port2", "")
        );
    }
    #[test]
    fn new_mouse_uses_startup_defaults_and_binding_survives() {
        let d = Identity {
            key: "a".into(),
            name: "Mouse".into(),
            instance: "".into(),
            interface: "".into(),
            container: None,
            serial: None,
        };
        let mut c = Config::default();
        let (id, new) = c.discover(&d, DEFAULT);
        assert!(new);
        c.profiles.get_mut(&id).unwrap().parameters.speed = 4;
        assert!(!c.discover(&d, DEFAULT).1);
        assert_eq!(c.profiles[&id].parameters.speed, 4);
    }
    #[test]
    fn precision_preserves_enabled_parameters() {
        let p = Parameters {
            speed: 6,
            acceleration: [3, 9, 2],
        };
        assert_eq!(p.with_precision(true), p);
        assert_eq!(p.with_precision(false).acceleration, [3, 9, 0]);
    }

    fn device(key: &str) -> Identity {
        Identity {
            key: key.into(),
            name: key.into(),
            instance: key.into(),
            interface: key.into(),
            container: None,
            serial: None,
        }
    }
    #[test]
    fn sole_mouse_policy_handles_removal_and_multiple_interfaces() {
        let mut devices = BTreeMap::new();
        assert_eq!(sole_device(&devices), None);
        devices.insert(10, device("a"));
        assert_eq!(sole_device(&devices), Some(10));
        devices.insert(11, device("a"));
        assert_eq!(sole_device(&devices), Some(10));
        devices.insert(20, device("b"));
        assert_eq!(sole_device(&devices), None);
        devices.remove(&10);
        devices.remove(&11);
        assert_eq!(sole_device(&devices), Some(20));
    }
    #[test]
    fn alternating_profiles_and_rebound_device_use_saved_settings() {
        let mut c = Config::default();
        let a = device("a");
        let b = device("b");
        let moved_a = device("a-new-port");
        c.discover(&a, DEFAULT);
        c.discover(&b, DEFAULT);
        c.discover(&moved_a, DEFAULT);
        let slow = Parameters {
            speed: 3,
            ..DEFAULT
        }
        .with_precision(false);
        c.profiles.get_mut("a").unwrap().parameters = slow;
        c.bindings.insert(moved_a.key.clone(), a.key.clone());
        let c: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        c.validate().unwrap();
        assert_eq!(c.clone().discover(&moved_a, DEFAULT), ("a".into(), false));
        let mut s = engine();
        for (key, expected) in [("a", slow), ("b", DEFAULT), ("a-new-port", slow)] {
            let id = &c.bindings[key];
            s.activate(id, c.profiles[id].parameters, false).unwrap();
            assert_eq!(s.system.value, expected);
        }
        assert_eq!(s.system.writes, 3);
    }
}
