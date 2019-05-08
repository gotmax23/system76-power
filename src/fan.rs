use std::io;
use sysfs_class::{SysClass, HwMon};

pub struct FanDaemon {
    curve: FanCurve,
    platforms: Vec<HwMon>,
    cpus: Vec<HwMon>,
}

impl FanDaemon {
    pub fn new() -> io::Result<FanDaemon> {
        //TODO: Support multiple hwmons for platform and cpu
        let mut platforms = Vec::new();
        let mut cpus = Vec::new();

        for hwmon in HwMon::all()? {
            if let Ok(name) = hwmon.name() {
                info!("hwmon: {}", name);

                match name.as_str() {
                    "system76" => (), //TODO: Support laptops
                    "system76_io" => platforms.push(hwmon),
                    "coretemp" | "k10temp" => cpus.push(hwmon),
                    _ => ()
                }
            }
        }

        if platforms.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "platform hwmon not found"
            ));
        }

        if cpus.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "cpu hwmon not found"
            ));
        }

        Ok(FanDaemon {
            curve: FanCurve::standard(),
            platforms,
            cpus,
        })
    }

    /// Get the maximum measured temperature from any CPU on the system, in thousandths Celsius
    /// Thousandths celsius is the standard Linux hwmon temperature unit
    pub fn get_temp(&self) -> Option<u32> {
        let mut temp_opt = None;
        for cpu in self.cpus.iter() {
            if let Ok(temp) = cpu.temp(1) {
                if let Ok(input) = temp.input() {
                    if temp_opt.map_or(true, |x| input > x) {
                        temp_opt = Some(input);
                    }
                }
            }
        }
        temp_opt
    }

    /// Get the correct duty cycle for a temperature in thousandths Celsius, from 0 to 255
    /// Thousandths celsius is the standard Linux hwmon temperature unit
    /// 0 to 255 is the standard Linux hwmon pwm unit
    pub fn get_duty(&self, temp: u32) -> Option<u8> {
        self.curve.get_duty((temp / 10) as i16).map(|duty| {
            (((duty as u32) * 255) / 10_000) as u8
        })
    }

    /// Set the current duty cycle, from 0 to 255
    /// 0 to 255 is the standard Linux hwmon pwm unit
    pub fn set_duty(&self, duty_opt: Option<u8>) {
        if let Some(duty) = duty_opt {
            let duty_str = format!("{}", duty);
            for platform in self.platforms.iter() {
                let _ = platform.write_file("pwm1_enable", "1");
                let _ = platform.write_file("pwm1", &duty_str);
                let _ = platform.write_file("pwm2", &duty_str);
            }
        } else {
            for platform in self.platforms.iter() {
                let _ = platform.write_file("pwm1_enable", "2");
            }
        }
    }

    /// Calculate the correct duty cycle and apply it to all fans
    pub fn step(&self) {
        self.set_duty(
            self.get_temp().and_then(|temp| {
                self.get_duty(temp)
            })
        )
    }
}

impl Drop for FanDaemon {
    fn drop(&mut self) {
        self.set_duty(None);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanPoint {
    // Temperature in hundredths of a degree, 10000 = 100C
    temp: i16,
    // duty in hundredths of a percent, 10000 = 100%
    duty: u16,
}

impl FanPoint {
    pub fn new(temp: i16, duty: u16) -> Self {
        Self {
            temp,
            duty
        }
    }

    /// Find the duty between two points and a given temperature, if the temperature
    /// lies within this range.
    fn get_duty_between_points(self, next: FanPoint, temp: i16) -> Option<u16> {
        // If the temp matches the next point, return the next point duty
        if temp == next.temp {
            return Some(next.duty);
        }

        // If the temp matches the previous point, return the previous point duty
        if temp == self.temp {
            return Some(self.duty);
        }

        // If the temp is in between the previous and next points, interpolate the duty
        if self.temp < temp && next.temp > temp {
            return Some(self.interpolate_duties(next, temp));
        }

        None
    }

    /// Interpolates the current duty with that of the given next point and temperature.
    fn interpolate_duties(self, next: FanPoint, temp: i16) -> u16 {
        let dtemp = next.temp - self.temp;
        let dduty = next.duty - self.duty;

        let slope = f32::from(dduty) / f32::from(dtemp);

        let temp_offset = temp - self.temp;
        let duty_offset = (slope * f32::from(temp_offset)).round();

        self.duty + (duty_offset as u16)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FanCurve {
    points: Vec<FanPoint>
}

impl FanCurve {
    /// Adds a point to the fan curve
    pub fn append(mut self, temp: i16, duty: u16) -> Self {
        self.points.push(FanPoint::new(temp, duty));
        self
    }

    /// The standard fan curve
    pub fn standard() -> Self {
        Self::default()
            .append(20_00, 30_00)
            .append(30_00, 35_00)
            .append(40_00, 42_50)
            .append(50_00, 52_50)
            .append(65_00, 10_000)
    }

    pub fn get_duty(&self, temp: i16) -> Option<u16> {
        // If the temp is less than the first point, return the first point duty
        if let Some(first) = self.points.first() {
            if temp < first.temp {
                return Some(first.duty);
            }
        }

        // Use when we upgrade to 1.28.0
        // for &[prev, next] in self.points.windows(2) {

        for window in self.points.windows(2) {
            let prev = window[0];
            let next = window[1];
            if let Some(duty) = prev.get_duty_between_points(next, temp) {
                return Some(duty);
            }
        }

        // If the temp is greater than the last point, return the last point duty
        if let Some(last) = self.points.last() {
            if temp > last.temp {
                return Some(last.duty);
            }
        }

        // If there are no points, return None
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duty_interpolation() {
        let fan_point = FanPoint::new(20_00, 30_00);
        let next_point = FanPoint::new(30_00, 35_00);

        assert_eq!(fan_point.get_duty_between_points(next_point, 1500), None);
        assert_eq!(fan_point.get_duty_between_points(next_point, 2000), Some(3000));
        assert_eq!(fan_point.get_duty_between_points(next_point, 3000), Some(3500));
        assert_eq!(fan_point.get_duty_between_points(next_point, 3250), None);
        assert_eq!(fan_point.get_duty_between_points(next_point, 3500), None);
    }

    #[test]
    fn standard_points() {
        let standard = FanCurve::standard();

        assert_eq!(standard.get_duty(0), Some(3000));
        assert_eq!(standard.get_duty(1000), Some(3000));
        assert_eq!(standard.get_duty(2000), Some(3000));
        assert_eq!(standard.get_duty(3000), Some(3500));
        assert_eq!(standard.get_duty(4000), Some(4250));
        assert_eq!(standard.get_duty(5000), Some(5250));
        assert_eq!(standard.get_duty(6000), Some(8417));
        assert_eq!(standard.get_duty(7000), Some(10000));
        assert_eq!(standard.get_duty(8000), Some(10000));
        assert_eq!(standard.get_duty(9000), Some(10000));
        assert_eq!(standard.get_duty(10000), Some(10000));
    }
}
