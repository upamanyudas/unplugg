use sha2::{Digest, Sha256};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/upamanyudas/unplugg/releases/latest";

pub fn latest_release(agent: &ureq::Agent) -> Option<serde_json::Value> {
    let response = agent
        .get(LATEST)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .call();
    let response = match response {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(_) => return None,
    };
    serde_json::from_reader(response.into_reader()).ok()
}

pub fn newer(tag: &str, current: &str) -> bool {
    fn parse(version: &str) -> Option<Vec<(bool, String)>> {
        version
            .split('.')
            .map(|part| {
                let part = part.trim();
                let (negative, digits) = match part.strip_prefix('-') {
                    Some(rest) => (true, rest),
                    None => (false, part.strip_prefix('+').unwrap_or(part)),
                };
                if digits.is_empty()
                    || digits.starts_with('_')
                    || digits.ends_with('_')
                    || digits.contains("__")
                    || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'_')
                {
                    return None;
                }
                let digits = digits.replace('_', "");
                let digits = digits.trim_start_matches('0');
                Some((negative && !digits.is_empty(), digits.to_string()))
            })
            .collect()
    }
    fn compare(a: &(bool, String), b: &(bool, String)) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (a.0, b.0) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (negative, _) => {
                let order = a.1.len().cmp(&b.1.len()).then_with(|| a.1.cmp(&b.1));
                if negative {
                    order.reverse()
                } else {
                    order
                }
            }
        }
    }
    match (parse(tag), parse(current)) {
        (Some(tag), Some(current)) => tag
            .iter()
            .zip(&current)
            .map(|(a, b)| compare(a, b))
            .find(|order| !order.is_eq())
            .unwrap_or_else(|| tag.len().cmp(&current.len()))
            .is_gt(),
        _ => false,
    }
}

pub fn should_offer(tag: &str, current: &str, skipped: &str, manual: bool) -> bool {
    let version = tag.trim_start_matches('v');
    newer(version, current) && (manual || version != skipped)
}

pub fn notes(text: &str) -> String {
    let mut headings = String::new();
    let mut chars = text.chars().peekable();
    let mut line_start = true;
    while let Some(c) = chars.next() {
        if line_start && c == '#' {
            while chars.peek() == Some(&'#') {
                chars.next();
            }
            line_start = false;
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                line_start = chars.next() == Some('\n');
            }
        } else {
            headings.push(c);
            line_start = c == '\n';
        }
    }
    let mut result = String::new();
    let mut chars = headings.chars().peekable();
    let mut line_start = true;
    while let Some(c) = chars.next() {
        if line_start && (c == '*' || c == '-') && chars.peek() == Some(&' ') {
            chars.next();
            result.push_str("• ");
            line_start = false;
        } else {
            result.push(c);
            line_start = c == '\n';
        }
    }
    result.replace("**", "")
}

pub fn verify_sha256(data: &[u8], checksum: &str) -> Result<(), &'static str> {
    let actual = format!("{:x}", Sha256::digest(data));
    if checksum.split_whitespace().next() == Some(actual.as_str()) {
        Ok(())
    } else {
        Err("The download didn't match its checksum.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_skip() {
        assert!(newer("1.10.0", "1.9.9"));
        assert!(newer("999999999999999999999999999999999", "2"));
        assert!(!newer("1.0", "1.0.0"));
        assert!(!should_offer("v1.2.0", "1.2.0", "", true));
        assert!(newer("1.0.0", "1.0"));
        assert!(!newer("1.2-rc1", "1.1"));
        assert!(!should_offer("v1.2.0", "1.1.0", "1.2.0", false));
        assert!(should_offer("v1.2.0", "1.1.0", "1.2.0", true));
    }

    #[test]
    fn release_notes() {
        assert_eq!(
            notes("## Heading\n* **One**\n- Two"),
            "Heading\n• One\n• Two"
        );
        assert_eq!(notes("#\n  Heading\nplain"), "Heading\nplain");
    }

    #[test]
    fn checksum() {
        assert!(verify_sha256(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  file"
        )
        .is_ok());
        assert!(verify_sha256(b"abc", "wrong").is_err());
    }
}
