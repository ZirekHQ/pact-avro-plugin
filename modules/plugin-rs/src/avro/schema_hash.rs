use md5::{Digest, Md5};

/// Mirrors AvroSchemaBase16Hash.scala: lowercase hex MD5 of the schema's
/// canonical text form.
pub fn base16_hash(schema_text: &str) -> String {
    let digest = Md5::digest(schema_text.as_bytes());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_input_to_its_known_md5_hex_digest() {
        // Well-known MD5 test vectors, not tied to any Avro schema shape.
        assert_eq!(base16_hash(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(base16_hash("test"), "098f6bcd4621d373cade4e832627b4f6");
    }

    #[test]
    fn hash_is_lowercase_hex() {
        let hash = base16_hash("{\"type\":\"record\"}");
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(hash.len(), 32);
    }
}
