use std::{fs, io};
use std::io::Write;
use std::process::{Command, Stdio};

use module::Module;
use pci::PciBus;
use sysfs_class::{PciDevice, SysClass};

use std::path::Path;

static MODPROBE_NVIDIA: &'static [u8] = br#"# Automatically generated by system76-power
"#;

static MODPROBE_INTEL: &'static [u8] = br#"# Automatically generated by system76-power
blacklist nouveau
blacklist nvidia
blacklist nvidia-drm
blacklist nvidia-modeset
alias nouveau off
alias nvidia off
alias nvidia-drm off
alias nvidia-modeset off
"#;

pub struct GraphicsDevice {
    functions: Vec<PciDevice>,
}

impl GraphicsDevice {
    pub fn new(functions: Vec<PciDevice>) -> GraphicsDevice {
        GraphicsDevice {
            functions
        }
    }

    pub fn exists(&self) -> bool {
        self.functions.iter().any(|func| func.path().exists())
    }

    pub unsafe fn unbind(&self) -> io::Result<()> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        info!("{}: Unbinding {}", driver.id(), func.id());
                        driver.unbind(&func)?;
                    },
                    Err(err) => match err.kind() {
                        io::ErrorKind::NotFound => (),
                        _ => return Err(err),
                    }
                }
            }
        }

        Ok(())
    }

    pub unsafe fn remove(&self) -> io::Result<()> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        error!("{}: in use by {}", func.id(), driver.id());
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "device in use"
                        ));
                    },
                    Err(err) => match err.kind() {
                        io::ErrorKind::NotFound => {
                            info!("{}: Removing", func.id());
                            func.remove()?;
                        },
                        _ => return Err(err),
                    }
                }
            } else {
                warn!("{}: Already removed", func.id());
            }
        }

        Ok(())
    }
}

pub struct Graphics {
    pub bus: PciBus,
    pub amd: Vec<GraphicsDevice>,
    pub intel: Vec<GraphicsDevice>,
    pub nvidia: Vec<GraphicsDevice>,
    pub other: Vec<GraphicsDevice>,
}

impl Graphics {
    pub fn new() -> io::Result<Graphics> {
        let bus = PciBus::new()?;

        info!("Rescanning PCI bus");
        bus.rescan()?;

        let devs = PciDevice::all()?;

        let functions = |parent: &PciDevice| -> Vec<PciDevice> {
            let mut functions = Vec::new();
            if let Some(parent_slot) = parent.id().split(".").next() {
                for func in devs.iter() {
                    if let Some(func_slot) = func.id().split(".").next() {
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
                        amd.push(GraphicsDevice::new(functions(&dev)));
                    }
                    0x10DE => {
                        info!("{}: NVIDIA graphics", dev.id());
                        nvidia.push(GraphicsDevice::new(functions(&dev)));
                    },
                    0x8086 => {
                        info!("{}: Intel graphics", dev.id());
                        intel.push(GraphicsDevice::new(functions(&dev)));
                    },
                    vendor => {
                        info!("{}: Other({:X}) graphics", dev.id(), vendor);
                        other.push(GraphicsDevice::new(functions(&dev)));
                    },
                },
                _ => ()
            }
        }

        Ok(Graphics {
            bus,
            amd,
            intel,
            nvidia,
            other,
        })
    }

    pub fn can_switch(&self) -> bool {
        !self.intel.is_empty() && !self.nvidia.is_empty()
    }

    pub fn get_vendor(&self) -> io::Result<String> {
        let modules = Module::all()?;
        let vendor = if modules.iter().any(|module| module.name == "nouveau" || module.name == "nvidia") {
            "nvidia".to_string()
        } else {
            "intel".to_string()
        };

        Ok(vendor)
    }

    pub fn set_vendor(&self, vendor: &str) -> io::Result<()> {
        if self.can_switch() {
            {
                let path = "/etc/modprobe.d/system76-power.conf";
                info!("Creating {}", path);
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)?;

                if vendor == "nvidia" {
                    file.write_all(MODPROBE_NVIDIA)?;
                } else {
                    file.write_all(MODPROBE_INTEL)?;
                }

                file.sync_all()?;
            }

            if vendor == "nvidia" {
                info!("Enabling nvidia-fallback.service");
                let status = Command::new("systemctl").arg("enable").arg("nvidia-fallback.service").status()?;
                if ! status.success() {
                    // Error is ignored in case this service is removed
                    error!("systemctl: failed with {}", status);
                }
            } else {
                info!("Disabling nvidia-fallback.service");
                let status = Command::new("systemctl").arg("disable").arg("nvidia-fallback.service").status()?;
                if ! status.success() {
                    // Error is ignored in case this service is removed
                    error!("systemctl: failed with {}", status);
                }
            }

            info!("Updating initramfs");            
            
            // Use Dracut or update-initramfs and return status
            let status;

            if Command::new("command").arg("-v").arg("dracut").stdout(Stdio::null()).status()?.success() {

                status = Command::new("dracut").arg("--force").status()?;

            } else if Command::new("command").arg("-v").arg("update-initramfs").stdout(Stdio::null()).status()?.success() {

                status = Command::new("update-initramfs").arg("-u").status()?;

            } else {
                // Tools not found. Raise an error.
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("update-initramfs: failed. No Dracut nor update-initfamfs found.")
                ));
            }


            if ! status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("update-initramfs: failed with {}", status)
                ));
            }

            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "does not have switchable graphics"
            ))
        }
    }

    pub fn get_power(&self) -> io::Result<bool> {
        if self.can_switch() {
            Ok(self.nvidia.iter().any(|dev| dev.exists()))
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "does not have switchable graphics"
            ))
        }
    }

    pub fn set_power(&self, power: bool) -> io::Result<()> {
        if self.can_switch() {
            if power {
                info!("Enabling graphics power");
                self.bus.rescan()?;
            } else {
                info!("Disabling graphics power");

                // Unbind NVIDIA graphics devices and their functions
                for dev in self.nvidia.iter() {
                    unsafe { dev.unbind()?; }
                }

                // Remove NVIDIA graphics devices and their functions
                for dev in self.nvidia.iter() {
                    unsafe { dev.remove()?; }
                }
            }
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "does not have switchable graphics"
            ))
        }
    }

    pub fn auto_power(&self) -> io::Result<()> {
        self.set_power(self.get_vendor()? == "nvidia")
    }
}
