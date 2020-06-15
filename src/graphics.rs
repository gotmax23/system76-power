use crate::{module::Module, pci::PciBus};
use std::{
    fs,
    io::{self, Write},
    iter::FromIterator,
    process::{self, ExitStatus},
};
use sysfs_class::{PciDevice, SysClass};

const MODPROBE_PATH: &str = "/etc/modprobe.d/system76-power.conf";

static MODPROBE_NVIDIA: &[u8] = br#"# Automatically generated by system76-power
"#;

static MODPROBE_HYBRID: &[u8] = br#"# Automatically generated by system76-power
# http://download.nvidia.com/XFree86/Linux-x86_64/435.17/README/dynamicpowermanagement.html
options nvidia NVreg_DynamicPowerManagement=0x02
blacklist i2c_nvidia_gpu
alias i2c_nvidia_gpu off
"#;

static MODPROBE_INTEGRATED: &[u8] = br#"# Automatically generated by system76-power
blacklist i2c_nvidia_gpu
blacklist nouveau
blacklist nvidia
blacklist nvidia-drm
blacklist nvidia-modeset
alias i2c_nvidia_gpu off
alias nouveau off
alias nvidia off
alias nvidia-drm off
alias nvidia-modeset off
"#;

const PRIME_DISCRETE_PATH: &str = "/etc/prime-discrete";

#[derive(Debug, Error)]
pub enum GraphicsDeviceError {
    #[error(display = "failed to execute {} command: {}", cmd, why)]
    Command { cmd: &'static str, why: io::Error },
    #[error(display = "{} in use by {}", func, driver)]
    DeviceInUse { func: String, driver: String },
    #[error(display = "failed to open system76-power modprobe file: {}", _0)]
    ModprobeFileOpen(io::Error),
    #[error(display = "failed to write to system76-power modprobe file: {}", _0)]
    ModprobeFileWrite(io::Error),
    #[error(display = "failed to fetch list of active kernel modules: {}", _0)]
    ModulesFetch(io::Error),
    #[error(display = "does not have switchable graphics")]
    NotSwitchable,
    #[error(display = "PCI driver error on {}: {}", device, why)]
    PciDriver { device: String, why: io::Error },
    #[error(display = "failed to get PRIME value: {}", _0)]
    PrimeModeRead(io::Error),
    #[error(display = "failed to set PRIME value: {}", _0)]
    PrimeModeWrite(io::Error),
    #[error(display = "failed to remove PCI device {}: {}", device, why)]
    Remove { device: String, why: io::Error },
    #[error(display = "failed to rescan PCI bus: {}", _0)]
    Rescan(io::Error),
    #[error(display = "failed to unbind {} on PCI driver {}: {}", func, driver, why)]
    Unbind { func: String, driver: String, why: io::Error },
    #[error(display = "update-initramfs failed with {} status", _0)]
    UpdateInitramfs(ExitStatus),
    #[error(display = "update-initramfs didn't found tools and failed with {} status", _0)]
    UpdateInitramfsNoTools(ExitStatus),
}

pub struct GraphicsDevice {
    id:        String,
    functions: Vec<PciDevice>,
}

impl GraphicsDevice {
    pub fn new(id: String, functions: Vec<PciDevice>) -> GraphicsDevice {
        GraphicsDevice { id, functions }
    }

    pub fn exists(&self) -> bool { self.functions.iter().any(|func| func.path().exists()) }

    pub unsafe fn unbind(&self) -> Result<(), GraphicsDeviceError> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        info!("{}: Unbinding {}", driver.id(), func.id());
                        driver.unbind(&func).map_err(|why| GraphicsDeviceError::Unbind {
                            driver: driver.id().to_owned(),
                            func: func.id().to_owned(),
                            why,
                        })?;
                    }
                    Err(why) => match why.kind() {
                        io::ErrorKind::NotFound => (),
                        _ => {
                            return Err(GraphicsDeviceError::PciDriver {
                                device: self.id.clone(),
                                why,
                            })
                        }
                    },
                }
            }
        }

        Ok(())
    }

    pub unsafe fn remove(&self) -> Result<(), GraphicsDeviceError> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        error!("{}: in use by {}", func.id(), driver.id());
                        return Err(GraphicsDeviceError::DeviceInUse {
                            func:   func.id().to_owned(),
                            driver: driver.id().to_owned(),
                        });
                    }
                    Err(why) => match why.kind() {
                        io::ErrorKind::NotFound => {
                            info!("{}: Removing", func.id());
                            func.remove().map_err(|why| GraphicsDeviceError::Remove {
                                device: self.id.clone(),
                                why,
                            })?;
                        }
                        _ => {
                            return Err(GraphicsDeviceError::PciDriver {
                                device: self.id.clone(),
                                why,
                            })
                        }
                    },
                }
            } else {
                warn!("{}: Already removed", func.id());
            }
        }

        Ok(())
    }
}

pub struct Graphics {
    pub bus:    PciBus,
    pub amd:    Vec<GraphicsDevice>,
    pub intel:  Vec<GraphicsDevice>,
    pub nvidia: Vec<GraphicsDevice>,
    pub other:  Vec<GraphicsDevice>,
}

impl Graphics {
    pub fn new() -> io::Result<Graphics> {
        let bus = PciBus::new()?;

        info!("Rescanning PCI bus");
        bus.rescan()?;

        let devs = PciDevice::all()?;

        let functions = |parent: &PciDevice| -> Vec<PciDevice> {
            let mut functions = Vec::new();
            if let Some(parent_slot) = parent.id().split('.').next() {
                for func in devs.iter() {
                    if let Some(func_slot) = func.id().split('.').next() {
                        if func_slot == parent_slot {
                            info!("{}: Function for {}", func.id(), parent.id());
                            functions.push(func.clone());
                        }
                    }
                }
            }
            functions
        };

        let mut amd = Vec::new();
        let mut intel = Vec::new();
        let mut nvidia = Vec::new();
        let mut other = Vec::new();
        for dev in devs.iter() {
            let c = dev.class()?;
            match (c >> 16) & 0xFF {
                0x03 => match dev.vendor()? {
                    0x1002 => {
                        info!("{}: AMD graphics", dev.id());
                        amd.push(GraphicsDevice::new(dev.id().to_owned(), functions(&dev)));
                    }
                    0x10DE => {
                        info!("{}: NVIDIA graphics", dev.id());
                        nvidia.push(GraphicsDevice::new(dev.id().to_owned(), functions(&dev)));
                    }
                    0x8086 => {
                        info!("{}: Intel graphics", dev.id());
                        intel.push(GraphicsDevice::new(dev.id().to_owned(), functions(&dev)));
                    }
                    vendor => {
                        info!("{}: Other({:X}) graphics", dev.id(), vendor);
                        other.push(GraphicsDevice::new(dev.id().to_owned(), functions(&dev)));
                    }
                },
                _ => (),
            }
        }

        Ok(Graphics { bus, amd, intel, nvidia, other })
    }

    pub fn can_switch(&self) -> bool {
        !self.nvidia.is_empty() && (!self.intel.is_empty() || !self.amd.is_empty())
    }

    fn get_prime_discrete() -> Result<String, GraphicsDeviceError> {
        fs::read_to_string(PRIME_DISCRETE_PATH)
            .map_err(GraphicsDeviceError::PrimeModeRead)
            .map(|mode| mode.trim().to_owned())
    }

    fn set_prime_discrete(mode: &str) -> Result<(), GraphicsDeviceError> {
        fs::write(PRIME_DISCRETE_PATH, mode)
            .map_err(GraphicsDeviceError::PrimeModeWrite)
    }

    pub fn get_vendor(&self) -> Result<String, GraphicsDeviceError> {
        let modules = Module::all().map_err(GraphicsDeviceError::ModulesFetch)?;
        let vendor =
            if modules.iter().any(|module| module.name == "nouveau" || module.name == "nvidia") {
                let mode = match Self::get_prime_discrete() {
                    Ok(m) => m,
                    Err(_) => "nvidia".to_string(),
                };

                if mode == "on-demand" {
                    "hybrid".to_string()
                } else {
                    "nvidia".to_string()
                }
            } else {
                "integrated".to_string()
            };

        Ok(vendor)
    }

    pub fn set_vendor(&self, vendor: &str) -> Result<(), GraphicsDeviceError> {
        self.switchable_or_fail()?;

        let mode = if vendor == "hybrid" {
            "on-demand\n"
        } else if vendor == "nvidia" {
            "on\n"
        } else {
            "off\n"
        };

        info!("Setting {} to {}", PRIME_DISCRETE_PATH, mode);
        Self::set_prime_discrete(mode)?;

        {
            info!("Creating {}", MODPROBE_PATH);

            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(MODPROBE_PATH)
                .map_err(GraphicsDeviceError::ModprobeFileOpen)?;

            let text = if vendor == "hybrid" {
                MODPROBE_HYBRID
            } else if vendor == "nvidia" {
                MODPROBE_NVIDIA
            } else {
                MODPROBE_INTEGRATED
            };

            file.write_all(text)
                .and_then(|_| file.sync_all())
                .map_err(GraphicsDeviceError::ModprobeFileWrite)?;
        }

        const SYSTEMCTL_CMD: &str = "systemctl";

        let action = if vendor == "nvidia" {
            info!("Enabling nvidia-fallback.service");
            "enable"
        } else {
            info!("Disabling nvidia-fallback.service");
            "disable"
        };

        let status = process::Command::new(SYSTEMCTL_CMD)
            .arg(action)
            .arg("nvidia-fallback.service")
            .status()
            .map_err(|why| GraphicsDeviceError::Command { cmd: SYSTEMCTL_CMD, why })?;

        if !status.success() {
            // Error is ignored in case this service is removed
            warn!("systemctl: failed with {} (not an error if service does not exist!)", status);
        }

        info!("Updating initramfs");
        

        const COMMAND_CMD: &str = "command";
        const UPDATE_DRACUT_CMD: &str = "dracut";
        const UPDATE_INITRAMFS_CMD: &str = "update-initramfs";

        if process::Command::new(COMMAND_CMD).arg("-v").arg(UPDATE_DRACUT_CMD).stdout(process::Stdio::null()).status().map_err(|why| GraphicsDeviceError::Command { cmd: UPDATE_DRACUT_CMD, why })?.success() {

            let status = process::Command::new(UPDATE_DRACUT_CMD)
                .arg("--force")
                .status()
                .map_err(|why| GraphicsDeviceError::Command { cmd: UPDATE_DRACUT_CMD, why })?;
            if ! status.success() {
                return Err(GraphicsDeviceError::UpdateInitramfs(status));
            }

            
        } else {

            let status = process::Command::new(UPDATE_INITRAMFS_CMD)
                .arg("-u")
                .status()
                .map_err(|why| GraphicsDeviceError::Command { cmd: UPDATE_INITRAMFS_CMD, why })?;
        
            if ! status.success() {
                return Err(GraphicsDeviceError::UpdateInitramfs(status));
            }

        }

        Ok(())
    }

    pub fn get_power(&self) -> Result<bool, GraphicsDeviceError> {
        self.switchable_or_fail()?;
        Ok(self.nvidia.iter().any(GraphicsDevice::exists))
    }

    pub fn set_power(&self, power: bool) -> Result<(), GraphicsDeviceError> {
        self.switchable_or_fail()?;

        if power {
            info!("Enabling graphics power");
            self.bus.rescan().map_err(GraphicsDeviceError::Rescan)?;
        } else {
            info!("Disabling graphics power");

            unsafe {
                // Unbind NVIDIA graphics devices and their functions
                let unbinds = self.nvidia.iter().map(|dev| dev.unbind());

                // Remove NVIDIA graphics devices and their functions
                let removes = self.nvidia.iter().map(|dev| dev.remove());

                Result::from_iter(unbinds.chain(removes))?;
            }
        }

        Ok(())
    }

    pub fn auto_power(&self) -> Result<(), GraphicsDeviceError> {
        let vendor = self.get_vendor()?;
        self.set_power(vendor == "nvidia" || vendor == "hybrid")
    }

    fn switchable_or_fail(&self) -> Result<(), GraphicsDeviceError> {
        if self.can_switch() {
            Ok(())
        } else {
            Err(GraphicsDeviceError::NotSwitchable)
        }
    }
}
