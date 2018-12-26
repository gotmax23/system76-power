use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct Profiles {
    #[default = "Profile::battery()"]
    #[serde(default)]
    pub battery: Profile,

    #[default = "Profile::balanced()"]
    #[serde(default)]
    pub balanced: Profile,

    #[default = "Profile::performance()"]
    #[serde(default)]
    pub performance: Profile,

    #[serde(flatten)]
    #[serde(default)]
    pub custom: HashMap<String, Profile>
}

impl Profiles {
    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        fn set_or_default(out: &mut Vec<u8>, profile: &str, current: &Profile, default: &Profile) {
            if current != default {
                out.extend_from_slice(format!("[profiles.{}]\n", profile).as_bytes());
                current.serialize_toml(out);
            } else {
                let backlight = default.backlight.as_ref().unwrap();
                let pstate = default.pstate.as_ref().unwrap();
                let radeon = default.radeon.as_ref().unwrap();
                out.extend_from_slice(
                    format!(
                        "# [profiles.{}]\n\
                         # backlight = {{ keyboard = {}, screen = {} }}\n\
                         # laptop_mode = {}\n\
                         # max_lost_work = {}\n\
                         # pstate = {{ min = {}, max = {}, turbo = {} }}\n\
                         # radeon = {{ profile = '{}', dpm_state = '{}', dpm_perf = '{}' }}\n\
                         # script = '$PATH'\n\n",
                         profile,
                         backlight.keyboard,
                         backlight.screen,
                         default.laptop_mode,
                         default.max_lost_work,
                         pstate.min,
                         pstate.max,
                         pstate.turbo,
                         radeon.profile,
                         radeon.dpm_state,
                         radeon.dpm_perf,
                    ).as_bytes()
                )
            }
        }

        set_or_default(out, "battery", &self.battery, &Profile::battery());
        set_or_default(out, "balanced", &self.balanced, &Profile::balanced());
        set_or_default(out, "performance", &self.performance, &Profile::performance());

        for (key, value) in &self.custom {
            out.extend_from_slice(format!("[profiles.{}]\n", key).as_bytes());
            value.serialize_toml(out);
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, SmartDefault)]
pub struct Profile {
    pub backlight: Option<ConfigBacklight>,
    pub laptop_mode: u8,
    pub max_lost_work: u32,
    pub pstate: Option<ConfigPState>,
    pub radeon: Option<ConfigRadeon>,
    pub script: Option<PathBuf>,
}

impl Profile {
    pub(crate) fn battery() -> Self {
        Self {
            backlight: Some(ConfigBacklight::battery()),
            laptop_mode: 2,
            max_lost_work: 15,
            pstate: Some(ConfigPState::battery()),
            radeon: Some(ConfigRadeon::battery()),
            script: None,
        }
    }

    pub(crate) fn balanced() -> Self {
        Self {
            backlight: Some(ConfigBacklight::balanced()),
            laptop_mode: 0,
            max_lost_work: 15,
            pstate: Some(ConfigPState::balanced()),
            radeon: Some(ConfigRadeon::balanced()),
            script: None,
        }
    }

    pub(crate) fn performance() -> Self {
        Self {
            backlight: Some(ConfigBacklight::performance()),
            laptop_mode: 0,
            max_lost_work: 15,
            pstate: Some(ConfigPState::performance()),
            radeon: Some(ConfigRadeon::performance()),
            script: None,
        }
    }

    pub(crate) fn serialize_toml(&self, out: &mut Vec<u8>) {
        if let Some(ref backlight) = self.backlight {
            backlight.serialize_toml(out);
        }

        if let Some(ref pstate) = self.pstate {
            pstate.serialize_toml(out);
        }

        let _ = match self.script {
            Some(ref script) => writeln!(out, "script = '{}'", script.display()),
            None => writeln!(out, "# script = '$PATH'"),
        };

        out.push(b'\n');
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ProfileKind {
    #[serde(rename = "battery")]
    Battery,
    #[serde(rename = "balanced")]
    Balanced,
    #[serde(rename = "performance")]
    Performance,
}

impl From<ProfileKind> for &'static str {
    fn from(profile: ProfileKind) -> Self {
        match profile {
            ProfileKind::Balanced => "balanced",
            ProfileKind::Battery => "battery",
            ProfileKind::Performance => "performance",
        }
    }
}
