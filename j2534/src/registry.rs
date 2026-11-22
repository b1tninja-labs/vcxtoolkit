//! The `PassThruSupport.04.04` registry record a J2534 app reads to find this
//! DLL — built as data, never written here.
//!
//! A J2534 application enumerates `HKLM\SOFTWARE\PassThruSupport.04.04\<vendor>`
//! (and the `WOW6432Node` twin for a 32-bit app on 64-bit Windows) and loads the
//! DLL named by `FunctionLibrary`. `docs/tool-boundary.md` already states this
//! crate is the one that *would* write that key; this module builds the exact
//! values and stops there. There is deliberately **no** code path that writes the
//! registry: the real `RegCreateKeyEx` / `RegSetValueEx` is an owner-run install
//! step ([`RegistryWrites::OWNER_RUN_NOTE`]), the same "build the plan, refuse to
//! act" stance `collect` takes for a vehicle read.

use crate::status::Protocol;

/// A registry value this record would set: a `(name, data)` pair under the key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Value {
    /// The value name, e.g. `FunctionLibrary`.
    pub name: &'static str,
    /// The value data, rendered as a string (the J2534 keys are `REG_SZ` or a
    /// `REG_DWORD` written as its decimal text here for inspection).
    pub data: String,
}

/// The full `PassThruSupport.04.04` record for this DLL: the key, its 64-bit
/// twin, and the values beneath them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportRecord {
    /// The native key path under `HKEY_LOCAL_MACHINE`.
    pub key: String,
    /// The `WOW6432Node` twin a 32-bit app reads on 64-bit Windows.
    pub wow6432_key: String,
    /// The values both keys carry.
    pub values: Vec<Value>,
}

/// The vendor sub-key name. The install key tail GM apps expect is
/// `"<product> - J2534"` (reads/tool-boundary notes record a real installer that
/// writes this shape).
pub const VENDOR: &str = "VXDIAG VCX-NANO - J2534";

/// The J2534 API version this DLL reports. The `04.04` in the key name is the
/// SAE J2534-1 (2004) API generation.
pub const API_VERSION: &str = "04.04";

/// Build the `PassThruSupport.04.04` record for `function_library` (the absolute
/// path to the built DLL).
///
/// `ProtocolsSupported` lists only ISO 15765 / CAN here — the GM diagnostic
/// channel this host is designed around — and the CAN/ISO 15765 capability flags
/// are set to `1`. The record is pure data; see [`RegistryWrites`] for why it is
/// never applied in-process.
#[must_use]
pub fn support_record(function_library: &str) -> SupportRecord {
    let base = "SOFTWARE\\PassThruSupport.04.04";
    let values = vec![
        Value {
            name: "Name",
            data: VENDOR.to_owned(),
        },
        Value {
            name: "Vendor",
            data: "VXDIAG (community vcxtoolkit host)".to_owned(),
        },
        Value {
            name: "FunctionLibrary",
            data: function_library.to_owned(),
        },
        Value {
            name: "ProtocolsSupported",
            data: format!(
                "CAN:ISO15765 (ids {},{})",
                Protocol::Can.id(),
                Protocol::Iso15765.id()
            ),
        },
        Value {
            name: "CAN",
            data: "1".to_owned(),
        },
        Value {
            name: "ISO15765",
            data: "1".to_owned(),
        },
        Value {
            name: "APIVersion",
            data: API_VERSION.to_owned(),
        },
    ];
    SupportRecord {
        key: format!("{base}\\{VENDOR}"),
        wow6432_key: format!("SOFTWARE\\WOW6432Node\\PassThruSupport.04.04\\{VENDOR}"),
        values,
    }
}

/// The write plan `register` / `unregister` would carry out — and the note that
/// they do not, in this crate, carry it out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryWrites {
    /// The record whose keys and values would be created (register) or whose keys
    /// would be deleted (unregister).
    pub record: SupportRecord,
    /// `true` for a register plan, `false` for an unregister plan.
    pub install: bool,
}

impl RegistryWrites {
    /// Why no function here writes the registry.
    pub const OWNER_RUN_NOTE: &'static str = "the real RegCreateKeyEx/RegSetValueEx write is an owner-run, Windows-only \
         install step; this crate builds the plan and does not modify HKLM";

    /// The plan to install this DLL's `PassThruSupport.04.04` keys.
    #[must_use]
    pub fn register(function_library: &str) -> Self {
        Self {
            record: support_record(function_library),
            install: true,
        }
    }

    /// The plan to remove this DLL's `PassThruSupport.04.04` keys.
    #[must_use]
    pub fn unregister(function_library: &str) -> Self {
        Self {
            record: support_record(function_library),
            install: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{API_VERSION, RegistryWrites, VENDOR, support_record};

    #[test]
    fn the_record_points_the_function_library_at_the_dll() {
        let record = support_record("C:\\vcxtoolkit\\vcxtoolkit_j2534.dll");
        assert_eq!(
            record.key,
            format!("SOFTWARE\\PassThruSupport.04.04\\{VENDOR}")
        );
        assert!(record.wow6432_key.contains("WOW6432Node"));
        let lib = record
            .values
            .iter()
            .find(|value| value.name == "FunctionLibrary")
            .expect("FunctionLibrary value");
        assert_eq!(lib.data, "C:\\vcxtoolkit\\vcxtoolkit_j2534.dll");
    }

    #[test]
    fn the_record_advertises_iso15765_and_the_api_version() {
        let record = support_record("x.dll");
        let names: Vec<&str> = record.values.iter().map(|value| value.name).collect();
        assert!(names.contains(&"ISO15765"));
        assert!(names.contains(&"CAN"));
        let api = record
            .values
            .iter()
            .find(|value| value.name == "APIVersion")
            .unwrap();
        assert_eq!(api.data, API_VERSION);
        assert_eq!(API_VERSION, "04.04");
    }

    #[test]
    fn register_and_unregister_share_the_record_and_differ_only_in_direction() {
        let install = RegistryWrites::register("x.dll");
        let remove = RegistryWrites::unregister("x.dll");
        assert!(install.install);
        assert!(!remove.install);
        assert_eq!(install.record, remove.record);
        assert!(RegistryWrites::OWNER_RUN_NOTE.contains("owner-run"));
    }
}
