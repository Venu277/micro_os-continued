use core::fmt;
use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Reads the current date and time from the motherboard's CMOS RTC chip.
pub fn read_rtc() -> DateTime {
    let mut cmos_address: Port<u8> = Port::new(0x70);
    let mut cmos_data: Port<u8> = Port::new(0x71);

    unsafe {
        // Wait until RTC update-in-progress flag (bit 7 of register 0x0A) is clear
        let mut timeout = 100_000usize;
        loop {
            cmos_address.write(0x0A);
            if (cmos_data.read() & 0x80) == 0 || timeout == 0 {
                break;
            }
            timeout -= 1;
            core::hint::spin_loop();
        }

        // Read raw register values
        cmos_address.write(0x00);
        let mut second = cmos_data.read();
        cmos_address.write(0x02);
        let mut minute = cmos_data.read();
        cmos_address.write(0x04);
        let mut hour = cmos_data.read();
        cmos_address.write(0x07);
        let mut day = cmos_data.read();
        cmos_address.write(0x08);
        let mut month = cmos_data.read();
        cmos_address.write(0x09);
        let mut year = cmos_data.read() as u16;

        // Try reading century from register 0x32
        cmos_address.write(0x32);
        let century = cmos_data.read();

        // Check format in Status Register B
        cmos_address.write(0x0B);
        let register_b = cmos_data.read();

        // Convert BCD to binary if register B bit 2 is NOT set
        let is_bcd = (register_b & 0x04) == 0;
        if is_bcd {
            second = bcd_to_binary(second);
            minute = bcd_to_binary(minute);
            hour = ((hour & 0x0F) + (((hour & 0x70) >> 4) * 10)) | (hour & 0x80);
            day = bcd_to_binary(day);
            month = bcd_to_binary(month);
            year = bcd_to_binary(year as u8) as u16;
        }

        // Convert 12 hour to 24 hour if bit 1 is NOT set and PM bit (0x80) is present
        let is_24_hr = (register_b & 0x02) != 0;
        if !is_24_hr && (hour & 0x80) != 0 {
            hour = ((hour & 0x7F) + 12) % 24;
        }

        let full_year = if century > 0 {
            let century_bin = if is_bcd {
                bcd_to_binary(century) as u16
            } else {
                century as u16
            };
            century_bin * 100 + year
        } else {
            2000 + year
        };

        DateTime {
            year: full_year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }
}

pub fn bcd_to_binary(val: u8) -> u8 {
    ((val >> 4) * 10) + (val & 0x0F)
}

/// Calculate system uptime from PIT timer ticks.
/// Default PIT ticks at ~18.2065 Hz (1193182 / 65536).
pub fn uptime_seconds() -> u64 {
    let ticks = crate::interrupts::timer_ticks();
    // (ticks * 65536) / 1193182
    (ticks as u128 * 65536 / 1193182) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bcd_conversion() {
        assert_eq!(bcd_to_binary(0x59), 59);
        assert_eq!(bcd_to_binary(0x00), 0);
        assert_eq!(bcd_to_binary(0x26), 26);
    }
}
