//! The firmware catalog the manager downloads.
//!
//! An item is an update only when its PID list contains the device and its
//! version dword is greater than the running version. This module does not
//! download a file and it does not write one.

use crate::image::version_is_newer;

/// One `ITEM` from the firmware catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareItem {
    /// `ID`.
    pub id: u32,
    /// `PID`, the comma-separated list.
    pub pids: Vec<u32>,
    /// `NAME`.
    pub name: String,
    /// `VERSION` in log order. A three-part version is padded with a trailing 0.
    pub version: [u8; 4],
    /// `SIZE`, in bytes.
    pub size: u64,
    /// `MD5`, the hex text from the catalog.
    pub md5: String,
    /// `URL` of the image.
    pub url: String,
}

/// Read every complete `ITEM`. An item missing a field is skipped.
#[must_use]
pub fn parse_catalog(xml: &str) -> Vec<FirmwareItem> {
    let mut items = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<ITEM>") {
        rest = &rest[start + "<ITEM>".len()..];
        let Some(end) = rest.find("</ITEM>") else {
            break;
        };
        if let Some(item) = one_item(&rest[..end]) {
            items.push(item);
        }
        rest = &rest[end + "</ITEM>".len()..];
    }
    items
}

/// The newest item whose PID list contains `pid`.
///
/// This is the image the catalog lists for the device, including when it matches
/// the running version.
#[must_use]
pub fn firmware_for(items: &[FirmwareItem], pid: u32) -> Option<&FirmwareItem> {
    items
        .iter()
        .filter(|item| item.pids.contains(&pid))
        .max_by_key(|item| u32::from_be_bytes(item.version))
}

/// The newest item whose PID list contains `pid` and whose version is newer than `running`.
#[must_use]
pub fn newer_firmware(items: &[FirmwareItem], pid: u32, running: [u8; 4]) -> Option<&FirmwareItem> {
    firmware_for(items, pid).filter(|item| version_is_newer(item.version, running))
}

fn one_item(body: &str) -> Option<FirmwareItem> {
    Some(FirmwareItem {
        id: tag(body, "ID")?.parse().ok()?,
        pids: pids(tag(body, "PID")?),
        name: tag(body, "NAME")?.to_owned(),
        version: version(tag(body, "VERSION")?)?,
        size: tag(body, "SIZE")?.parse().ok()?,
        md5: tag(body, "MD5")?.to_owned(),
        url: tag(body, "URL")?.to_owned(),
    })
    .filter(|item| !item.name.is_empty() && !item.pids.is_empty() && !item.url.is_empty())
}

fn tag<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("<{name}>");
    let end = format!("</{name}>");
    let from = body.find(&start)? + start.len();
    let rest = &body[from..];
    let to = rest.find(&end)?;
    Some(rest[..to].trim())
}

fn pids(text: &str) -> Vec<u32> {
    text.split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect()
}

fn version(text: &str) -> Option<[u8; 4]> {
    let mut out = [0_u8; 4];
    let mut count = 0;
    for part in text.split('.') {
        if count == 4 {
            return None;
        }
        out[count] = part.parse().ok()?;
        count += 1;
    }
    if count == 0 { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::{firmware_for, newer_firmware, parse_catalog};

    const CATALOG: &str = r"
        <FW>
            <ITEM>
                <ID>3</ID>
                <PID>836,837,843,844</PID>
                <NAME>VCIA</NAME>
                <VERSION>1.9.4.2</VERSION>
                <SIZE>191770</SIZE>
                <MD5>25A045E15791689C721A9235C24E59F8</MD5>
                <URL>http://www.vxdiag.net/managecenter/fws/vcxa.bin</URL>
            </ITEM>
            <ITEM>
                <ID>1</ID>
                <PID>1043,1049</PID>
                <NAME>VCI</NAME>
                <VERSION>1.9.6.0</VERSION>
                <SIZE>187965</SIZE>
                <MD5>AA576E1BECF2AC3D6D572F00A7619696</MD5>
                <URL>http://www.vxdiag.net/managecenter/fws/vcx.bin</URL>
            </ITEM>
            <ITEM>
                <ID>2</ID>
                <PID>0</PID>
                <NAME>DoIP</NAME>
                <VERSION>1.9.2</VERSION>
                <SIZE>7077892</SIZE>
                <MD5>C2B9B9D75F5EDA94BDBEE92251A2AB50</MD5>
                <URL>http://www.vxdiag.net/managecenter/fws/openwrt.bin</URL>
            </ITEM>
        </FW>
    ";

    #[test]
    fn vcia_1_9_4_2_is_not_newer_than_the_running_nano() {
        let items = parse_catalog(CATALOG);
        assert!(newer_firmware(&items, 837, [1, 9, 4, 2]).is_none());
        let older = newer_firmware(&items, 837, [1, 9, 4, 1]).expect("1.9.4.2");
        assert_eq!(older.name, "VCIA");
        assert_eq!(older.version, [1, 9, 4, 2]);
    }

    #[test]
    fn the_matching_catalog_image_is_kept() {
        let items = parse_catalog(CATALOG);
        let image = firmware_for(&items, 837).expect("vcia");
        assert_eq!(image.name, "VCIA");
        assert_eq!(image.version, [1, 9, 4, 2]);
        assert!(newer_firmware(&items, 837, image.version).is_none());
        assert!(firmware_for(&items, 9999).is_none());
    }

    #[test]
    fn vci_1_9_6_0_is_newer_for_its_own_pids() {
        let items = parse_catalog(CATALOG);
        let vci = newer_firmware(&items, 1043, [1, 9, 4, 2]).expect("1.9.6.0");
        assert_eq!(vci.version, [1, 9, 6, 0]);
        assert!(newer_firmware(&items, 837, [1, 0, 0, 0]).is_some_and(|item| item.name == "VCIA"));
        assert_eq!(
            parse_catalog(CATALOG)
                .iter()
                .find(|item| item.name == "DoIP")
                .map(|item| item.version),
            Some([1, 9, 2, 0])
        );
    }
}
