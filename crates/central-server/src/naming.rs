use anyhow::{Result, anyhow};
use regex::Regex;

const CANONICAL_TAG_PATTERN: &str =
    r"^[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,16}\.[A-Z0-9_]{2,8}\.[A-Z0-9_]{2,8}$";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTagName {
    pub site: String,
    pub area: String,
    pub unit: String,
    pub device: String,
    pub signal: String,
    pub attribute: String,
}

pub fn validate_canonical_tag_name(value: &str) -> Result<()> {
    let re = Regex::new(CANONICAL_TAG_PATTERN).expect("valid canonical pattern");
    if !re.is_match(value) {
        return Err(anyhow!(
            "invalid canonical tag name '{}'. expected format SITE.AREA.UNIT.DEVICE.SIGNAL.ATTRIBUTE (uppercase, alnum or underscore)",
            value
        ));
    }
    Ok(())
}

pub fn parse_canonical_tag_name(value: &str) -> Result<CanonicalTagName> {
    validate_canonical_tag_name(value)?;
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 6 {
        return Err(anyhow!(
            "invalid canonical tag name '{}': expected 6 segments",
            value
        ));
    }

    Ok(CanonicalTagName {
        site: parts[0].to_string(),
        area: parts[1].to_string(),
        unit: parts[2].to_string(),
        device: parts[3].to_string(),
        signal: parts[4].to_string(),
        attribute: parts[5].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_canonical_tag_name() {
        let v = "PLANTA1.CALDERA.U01.PT101.PRES.PV";
        assert!(validate_canonical_tag_name(v).is_ok());
        let parsed = parse_canonical_tag_name(v).unwrap();
        assert_eq!(parsed.site, "PLANTA1");
        assert_eq!(parsed.area, "CALDERA");
        assert_eq!(parsed.unit, "U01");
        assert_eq!(parsed.device, "PT101");
        assert_eq!(parsed.signal, "PRES");
        assert_eq!(parsed.attribute, "PV");
    }

    #[test]
    fn rejects_lowercase_or_spaces() {
        assert!(validate_canonical_tag_name("planta1.CALDERA.U01.PT101.PRES.PV").is_err());
        assert!(validate_canonical_tag_name("PLANTA1.CALDERA.U01.PT 101.PRES.PV").is_err());
    }

    #[test]
    fn rejects_wrong_segment_count() {
        assert!(validate_canonical_tag_name("PLANTA1.CALDERA.U01.PT101.PRES").is_err());
        assert!(validate_canonical_tag_name("PLANTA1.CALDERA.U01.PT101.PRES.PV.EXTRA").is_err());
    }
}
