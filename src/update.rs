//! The firmware-update steps, as a description.
//!
//! `plan_firmware` names the steps `VCX_DFU_AutoUpdate` takes. [`hold`] keeps a
//! step that stays in this process and refuses a step that would be written to
//! the dongle. Nothing here opens a port. Ordinals 20, 21, and 22 are not
//! steps: their command bytes were not recovered.

use crate::Command;
use crate::dfu::{
    BOOT_MODE, BOOT_POLLS, Built, DFU_NAME_OFFSET, DFU_SERIAL_OFFSET, LEAVE_MODE, POLL_WAIT,
    dfu_get_info, erase_chip, flash_length_fits, get_mode, in_boot, set_mode,
};
use crate::image::{ImageError, file_is_newer, read_header, read_pages};

/// One step of the update the DLL runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateStep {
    /// The packed records [`read_pages`](crate::read_pages) accepted.
    Pages {
        /// `Head.PageNum`.
        count: u32,
    },
    /// The dword compare at `0x7B2F9905`.
    ///
    /// `device_is_greater` is the log `Device FwVersion is New!`. The DLL falls
    /// through that log. A greater device version does not remove later steps.
    Version {
        /// The file dword is greater than the device dword.
        file_is_newer: bool,
        /// The device dword is greater than the file dword.
        device_is_greater: bool,
    },
    /// A command whose payload is fully specified. Channel byte 3 is 0.
    Send(Built),
    /// `GetMode` again, until the reply byte is [`BOOT_MODE`](crate::BOOT_MODE).
    ///
    /// `initial` is the counter stored before the first pass. Each pass calls
    /// `Sleep` for `delay_ms` and then `GetMode`. The pass repeats while the
    /// counter before the decrement was not 0.
    BootPoll {
        /// Counter stored before the first pass.
        initial: u8,
        /// `Sleep` argument, in milliseconds.
        delay_ms: u32,
    },
    /// Eight bytes at [`DFU_NAME_OFFSET`] of the DFU `GetInfo` buffer.
    ///
    /// The reply was not read, so the step stores the offset and not a name.
    Name {
        /// Byte offset in that buffer.
        offset: usize,
    },
    /// `WrFlash` once per page. `length` is the dword `AutoUpdate` pushes.
    ///
    /// That dword is the first dword of the image, `0xA5A5A5A5` for an image
    /// this crate accepts. The page length is the argument of the rewrite, not
    /// of this call.
    Flash {
        /// The length pushed to ordinal 37.
        length: u32,
        /// How many pages the loop walks.
        pages: u32,
    },
    /// `WrSerial` with length 8.
    ///
    /// When the third argument is 0, the pointer is the DFU `GetInfo` buffer
    /// at `offset`. The step does not carry the bytes: the reply is not in hand.
    Serial {
        /// Offset of the 8 bytes inside the buffer `GetInfo` fills.
        offset: usize,
    },
}

/// A step that does not leave this process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kept {
    /// The page count was read from the image.
    Pages {
        /// `Head.PageNum`.
        count: u32,
    },
    /// The version compare was computed from the arguments.
    Version {
        /// The file dword is greater.
        file_is_newer: bool,
        /// The device dword is greater.
        device_is_greater: bool,
    },
    /// The name offset is known. The name bytes are not.
    Name {
        /// Byte offset in the DFU `GetInfo` buffer.
        offset: usize,
    },
}

/// A step that was not carried out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refused {
    /// The step is a frame the DLL would send.
    WouldSend {
        /// Header byte 2.
        command: Command,
        /// Payload length. The bytes stay on the step.
        payload_len: usize,
    },
    /// `WrFlash`. `fits` is whether `length` is at most `0x1026`.
    Flash {
        /// The length `AutoUpdate` pushes.
        length: u32,
        /// [`flash_length_fits`](crate::flash_length_fits) of that length.
        fits: bool,
    },
    /// `WrSerial`. The 8 bytes are a `GetInfo` field, and this step does not send them.
    Serial,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WouldSend {
                command,
                payload_len,
            } => {
                write!(
                    f,
                    "update step {:#04x} ({} payload bytes) is not sent",
                    command.code(),
                    payload_len
                )
            }
            Self::Flash { length, fits } => {
                write!(f, "WrFlash length {length:#010x} is not sent (fits={fits})")
            }
            Self::Serial => f.write_str("WrSerial is not sent"),
        }
    }
}

impl std::error::Error for Refused {}

/// The ordered steps for one image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePlan {
    /// In the order `VCX_DFU_AutoUpdate` reaches them.
    pub steps: Vec<UpdateStep>,
}

/// Describe the update.
///
/// `device_version` is four bytes in log order, the same order as
/// [`DeviceInfo::fw_version`](crate::DeviceInfo::fw_version). `mode` is a
/// `GetMode` reply byte already in hand. [`None`] means that byte was not read,
/// so the plan includes the path taken when the byte is not `0xA5`.
///
/// # Errors
///
/// Returns the [`ImageError`] from [`read_header`](crate::read_header) or
/// [`read_pages`](crate::read_pages).
#[must_use = "an update plan that is dropped was not described"]
pub fn plan_firmware(
    image: &[u8],
    device_version: [u8; 4],
    mode: Option<u8>,
) -> Result<UpdatePlan, ImageError> {
    let header = read_header(image)?;
    let pages = read_pages(image)?;
    let Ok(count) = u32::try_from(pages.len()) else {
        return Err(ImageError::Record);
    };
    let mut steps = Vec::new();
    steps.push(UpdateStep::Pages { count });
    steps.push(UpdateStep::Send(get_mode()));
    if !mode.is_some_and(in_boot) {
        steps.push(UpdateStep::Send(Built {
            command: Command::GetInfo,
            payload: Vec::new(),
        }));
        steps.push(UpdateStep::Version {
            file_is_newer: file_is_newer(&header, device_version),
            device_is_greater: u32::from_be_bytes(device_version)
                > u32::from_be_bytes(header.version),
        });
        steps.push(UpdateStep::Send(set_mode(BOOT_MODE)));
        steps.push(UpdateStep::BootPoll {
            initial: BOOT_POLLS,
            delay_ms: POLL_WAIT,
        });
    }
    steps.push(UpdateStep::Send(dfu_get_info()));
    steps.push(UpdateStep::Name {
        offset: DFU_NAME_OFFSET,
    });
    steps.push(UpdateStep::Send(erase_chip()));
    if count > 0 {
        steps.push(UpdateStep::Flash {
            length: u32::from_le_bytes(header.format),
            pages: count,
        });
    }
    steps.push(UpdateStep::Serial {
        offset: DFU_SERIAL_OFFSET,
    });
    steps.push(UpdateStep::Send(set_mode(LEAVE_MODE)));
    Ok(UpdatePlan { steps })
}

/// Keep a local step. Refuse a step that would be sent, erased, or flashed.
///
/// # Errors
///
/// Returns [`Refused`] after matching the step. A [`UpdateStep::Pages`],
/// [`UpdateStep::Version`], or [`UpdateStep::Name`] is kept.
#[must_use = "a refusal that is dropped was not checked"]
pub fn hold(step: &UpdateStep) -> Result<Kept, Refused> {
    match step {
        UpdateStep::Pages { count } => Ok(Kept::Pages { count: *count }),
        UpdateStep::Version {
            file_is_newer,
            device_is_greater,
        } => Ok(Kept::Version {
            file_is_newer: *file_is_newer,
            device_is_greater: *device_is_greater,
        }),
        UpdateStep::Name { offset } => Ok(Kept::Name { offset: *offset }),
        UpdateStep::Send(built) => Err(Refused::WouldSend {
            command: built.command,
            payload_len: built.payload.len(),
        }),
        UpdateStep::BootPoll { .. } => Err(Refused::WouldSend {
            command: Command::DfuGetMode,
            payload_len: 0,
        }),
        UpdateStep::Flash { length, .. } => Err(Refused::Flash {
            length: *length,
            fits: flash_length_fits(*length),
        }),
        UpdateStep::Serial { .. } => Err(Refused::Serial),
    }
}

#[cfg(test)]
mod tests {
    use super::{DFU_SERIAL_OFFSET, Kept, Refused, UpdateStep, hold, plan_firmware};
    use crate::{Command, ImageError};

    fn image(pages: u32) -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x24];
        bytes[0..4].copy_from_slice(&[0xA5; 4]);
        bytes[4..12].copy_from_slice(b"VCX-At40");
        bytes[0x0C..0x10].copy_from_slice(&0x0109_0100_u32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&pages.to_le_bytes());
        for _ in 0..pages {
            bytes.extend_from_slice(&4_u32.to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&[1, 2, 3, 4]);
        }
        bytes
    }

    fn commands(plan: &super::UpdatePlan) -> Vec<(Command, Vec<u8>)> {
        plan.steps
            .iter()
            .filter_map(|step| match step {
                UpdateStep::Send(built) => Some((built.command, built.payload.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_greater_device_version_still_plans_the_erase() {
        let plan = plan_firmware(&image(1), [1, 9, 4, 2], None).unwrap();
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            UpdateStep::Version {
                file_is_newer: false,
                device_is_greater: true,
            }
        )));
        let sent = commands(&plan);
        assert!(sent.contains(&(Command::DfuErChip, Vec::new())));
        assert!(sent.contains(&(Command::DfuSetMode, vec![0xA5])));
        assert!(sent.contains(&(Command::DfuSetMode, vec![0])));
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            UpdateStep::Flash {
                length: 0xA5A5_A5A5,
                pages: 1,
            }
        )));
        assert!(plan.steps.contains(&UpdateStep::Serial {
            offset: DFU_SERIAL_OFFSET,
        }));
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            UpdateStep::BootPoll {
                initial: 10,
                delay_ms: 0x1F4,
            }
        )));
    }

    #[test]
    fn an_equal_version_still_plans_the_erase() {
        let plan = plan_firmware(&image(1), [1, 9, 1, 0], None).unwrap();
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            UpdateStep::Version {
                file_is_newer: false,
                device_is_greater: false,
            }
        )));
        assert!(commands(&plan).contains(&(Command::DfuErChip, Vec::new())));
    }

    #[test]
    fn boot_mode_skips_the_enter_sequence() {
        let plan = plan_firmware(&image(1), [1, 9, 4, 2], Some(0xA5)).unwrap();
        let sent = commands(&plan);
        assert!(
            !sent.iter().any(|(command, payload)| {
                *command == Command::DfuSetMode && payload == &[0xA5]
            })
        );
        assert!(!sent.iter().any(|(command, _)| *command == Command::GetInfo));
        assert!(
            !plan
                .steps
                .iter()
                .any(|step| matches!(step, UpdateStep::Version { .. }))
        );
        assert!(
            !plan
                .steps
                .iter()
                .any(|step| matches!(step, UpdateStep::BootPoll { .. }))
        );
        assert!(sent.contains(&(Command::DfuGetInfo, Vec::new())));
        assert!(sent.contains(&(Command::DfuErChip, Vec::new())));
        assert!(sent.contains(&(Command::DfuSetMode, vec![0])));
    }

    #[test]
    fn hold_keeps_the_local_steps_and_refuses_the_wire() {
        let plan = plan_firmware(&image(1), [1, 9, 4, 2], None).unwrap();
        let mut kept_pages = false;
        let mut kept_version = false;
        let mut refused_erase = false;
        let mut refused_flash = false;
        let mut refused_serial = false;
        for step in &plan.steps {
            match hold(step) {
                Ok(Kept::Pages { count }) => {
                    assert_eq!(count, 1);
                    kept_pages = true;
                }
                Ok(Kept::Version {
                    file_is_newer: false,
                    device_is_greater: true,
                }) => kept_version = true,
                Ok(Kept::Name { offset }) => assert_eq!(offset, 0x20),
                Err(Refused::WouldSend {
                    command: Command::DfuErChip,
                    payload_len: 0,
                }) => refused_erase = true,
                Err(Refused::WouldSend { .. }) => {}
                Err(Refused::Flash { length, fits }) => {
                    assert_eq!((length, fits), (0xA5A5_A5A5, false));
                    refused_flash = true;
                }
                Err(Refused::Serial) => refused_serial = true,
                Ok(other) => panic!("unexpected keep {other:?}"),
            }
        }
        assert!(kept_pages && kept_version && refused_erase && refused_flash && refused_serial);
    }

    #[test]
    fn a_short_flash_length_is_still_refused_and_it_fits() {
        let step = UpdateStep::Flash {
            length: 4,
            pages: 1,
        };
        assert_eq!(
            hold(&step),
            Err(Refused::Flash {
                length: 4,
                fits: true
            })
        );
    }

    #[test]
    fn a_broken_record_is_not_a_plan() {
        let mut bytes = image(1);
        bytes.truncate(0x24);
        assert_eq!(
            plan_firmware(&bytes, [1, 9, 4, 2], None),
            Err(ImageError::Record)
        );
    }
}
