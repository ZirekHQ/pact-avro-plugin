use crate::avro::schema::SchemaCtx;
use apache_avro::schema::{InnerDecimalSchema, Schema, UuidSchema};

type Rest<'b> = Result<&'b [u8], String>;
type Entry = (Vec<u8>, Vec<u8>);

/// Rewrites an Avro datum so every map block lists its entries in ascending key order.
pub fn sort_map_entries<'a>(
    ctx: &SchemaCtx<'a>,
    schema: &'a Schema,
    datum: &[u8],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(datum.len());
    match rewrite(ctx, schema, datum, &mut out)? {
        [] => Ok(out),
        rest => Err(format!("{} trailing bytes after avro datum", rest.len())),
    }
}

fn read_long(input: &[u8]) -> Result<(i64, &[u8]), String> {
    let last = input
        .iter()
        .take(10)
        .position(|byte| byte & 0x80 == 0)
        .ok_or_else(|| "truncated variable-length integer".to_string())?;
    if last == 9 && input[9] > 1 {
        return Err("variable-length integer exceeds 64 bits".to_string());
    }
    let raw = input[..=last]
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, byte)| {
            acc | u64::from(byte & 0x7f) << (7 * i)
        });
    Ok((
        ((raw >> 1) as i64) ^ -((raw & 1) as i64),
        &input[last + 1..],
    ))
}

fn write_long(value: i64, out: &mut Vec<u8>) {
    let mut raw = ((value << 1) ^ (value >> 63)) as u64;
    while raw >= 0x80 {
        out.push((raw & 0x7f) as u8 | 0x80);
        raw >>= 7;
    }
    out.push(raw as u8);
}

fn copy<'b>(input: &'b [u8], count: usize, out: &mut Vec<u8>) -> Rest<'b> {
    input
        .split_at_checked(count)
        .map(|(head, rest)| {
            out.extend_from_slice(head);
            rest
        })
        .ok_or_else(|| format!("expected {count} bytes but only {} remain", input.len()))
}

fn copy_long<'b>(input: &'b [u8], out: &mut Vec<u8>) -> Rest<'b> {
    read_long(input).and_then(|(_, rest)| copy(input, input.len() - rest.len(), out))
}

fn copy_sized<'b>(input: &'b [u8], out: &mut Vec<u8>) -> Rest<'b> {
    let (length, rest) = read_long(input)?;
    let size = usize::try_from(length).map_err(|_| format!("negative length {length}"))?;
    out.extend_from_slice(&input[..input.len() - rest.len()]);
    copy(rest, size, out)
}

fn read_block_count(input: &[u8]) -> Result<(usize, &[u8]), String> {
    let (count, rest) = read_long(input)?;
    usize::try_from(count)
        .map(|count| (count, rest))
        .map_err(|_| format!("unsupported block count {count}"))
}

fn rewrite<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    schema: &'a Schema,
    input: &'b [u8],
    out: &mut Vec<u8>,
) -> Rest<'b> {
    match ctx.resolve(schema) {
        Schema::Null => Ok(input),
        Schema::Boolean => copy(input, 1, out),
        Schema::Float => copy(input, 4, out),
        Schema::Double => copy(input, 8, out),
        Schema::Fixed(fixed) | Schema::Duration(fixed) => copy(input, fixed.size, out),
        Schema::Decimal(decimal) => match &decimal.inner {
            InnerDecimalSchema::Bytes => copy_sized(input, out),
            InnerDecimalSchema::Fixed(fixed) => copy(input, fixed.size, out),
        },
        Schema::Uuid(UuidSchema::Fixed(fixed)) => copy(input, fixed.size, out),
        Schema::Int
        | Schema::Long
        | Schema::Enum(_)
        | Schema::Date
        | Schema::TimeMillis
        | Schema::TimeMicros
        | Schema::TimestampMillis
        | Schema::TimestampMicros
        | Schema::TimestampNanos
        | Schema::LocalTimestampMillis
        | Schema::LocalTimestampMicros
        | Schema::LocalTimestampNanos => copy_long(input, out),
        Schema::Bytes
        | Schema::String
        | Schema::BigDecimal
        | Schema::Uuid(UuidSchema::Bytes | UuidSchema::String) => copy_sized(input, out),
        Schema::Record(record) => record
            .fields
            .iter()
            .try_fold(input, |rest, field| rewrite(ctx, &field.schema, rest, out)),
        Schema::Union(union) => rewrite_union(ctx, union.variants(), input, out),
        Schema::Array(array) => rewrite_array(ctx, &array.items, input, out),
        Schema::Map(map) => rewrite_map(ctx, &map.types, input, out),
        other => Err(format!("unsupported schema {other:?}")),
    }
}

fn rewrite_union<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    variants: &'a [Schema],
    input: &'b [u8],
    out: &mut Vec<u8>,
) -> Rest<'b> {
    let (index, rest) = read_long(input)?;
    let variant = usize::try_from(index)
        .ok()
        .and_then(|index| variants.get(index))
        .ok_or_else(|| format!("union index {index} out of range"))?;
    write_long(index, out);
    rewrite(ctx, variant, rest, out)
}

fn rewrite_array<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    items: &'a Schema,
    input: &'b [u8],
    out: &mut Vec<u8>,
) -> Rest<'b> {
    let mut rest = input;
    loop {
        let (count, after_count) = read_block_count(rest)?;
        write_long(count as i64, out);
        if count == 0 {
            return Ok(after_count);
        }
        rest = (0..count).try_fold(after_count, |rest, _| rewrite(ctx, items, rest, out))?;
    }
}

fn read_entry<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    values: &'a Schema,
    input: &'b [u8],
    entries: &mut Vec<Entry>,
) -> Rest<'b> {
    let mut encoded = Vec::new();
    let after_key = copy_sized(input, &mut encoded)?;
    let key = read_long(&encoded)?.1.to_vec();
    let rest = rewrite(ctx, values, after_key, &mut encoded)?;
    entries.push((key, encoded));
    Ok(rest)
}

fn read_entries<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    values: &'a Schema,
    input: &'b [u8],
    entries: &mut Vec<Entry>,
) -> Rest<'b> {
    let mut rest = input;
    loop {
        let (count, after_count) = read_block_count(rest)?;
        if count == 0 {
            return Ok(after_count);
        }
        rest = (0..count).try_fold(after_count, |rest, _| {
            read_entry(ctx, values, rest, entries)
        })?;
    }
}

fn rewrite_map<'a, 'b>(
    ctx: &SchemaCtx<'a>,
    values: &'a Schema,
    input: &'b [u8],
    out: &mut Vec<u8>,
) -> Rest<'b> {
    let mut entries = Vec::new();
    let rest = read_entries(ctx, values, input, &mut entries)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    if !entries.is_empty() {
        write_long(entries.len() as i64, out);
        entries.iter().for_each(|(_, encoded)| out.extend(encoded));
    }
    out.push(0);
    Ok(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_str;

    fn map_schema() -> Schema {
        parse_str(r#"{"type":"map","values":"int"}"#).unwrap()
    }

    #[test]
    fn zigzag_varints_round_trip() {
        [
            0,
            1,
            -1,
            63,
            -64,
            64,
            i64::from(i32::MAX),
            i64::MIN,
            i64::MAX,
        ]
        .into_iter()
        .for_each(|value| {
            let mut bytes = Vec::new();
            write_long(value, &mut bytes);
            assert_eq!(read_long(&bytes).unwrap(), (value, &[][..]));
        });
    }

    #[test]
    fn multi_block_maps_merge_into_one_sorted_block() {
        let schema = map_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let two_blocks = [0x02, 0x02, b'b', 0x02, 0x02, 0x02, b'a', 0x04, 0x00];
        assert_eq!(
            sort_map_entries(&ctx, &schema, &two_blocks).unwrap(),
            [0x04, 0x02, b'a', 0x04, 0x02, b'b', 0x02, 0x00]
        );
    }

    #[test]
    fn truncated_data_is_an_error() {
        let schema = map_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert!(sort_map_entries(&ctx, &schema, &[0x04, 0x02, b'a']).is_err());
    }

    #[test]
    fn negative_block_counts_are_an_error() {
        let schema = map_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert!(sort_map_entries(&ctx, &schema, &[0x01, 0x00, 0x00]).is_err());
    }

    #[test]
    fn bytes_after_the_datum_are_an_error() {
        let schema = map_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert_eq!(
            sort_map_entries(&ctx, &schema, &[0x00, 0x07]).unwrap_err(),
            "1 trailing bytes after avro datum"
        );
    }

    #[test]
    fn varints_wider_than_64_bits_are_an_error() {
        let overflowing = [[0x80; 9].as_slice(), &[0x02]].concat();
        assert!(read_long(&overflowing).is_err());
    }

    #[test]
    fn arrays_with_many_blocks_do_not_exhaust_the_stack() {
        let schema = parse_str(r#"{"type":"array","items":"int"}"#).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let mut datum = [0x02, 0x00].repeat(100_000);
        datum.push(0x00);
        assert_eq!(sort_map_entries(&ctx, &schema, &datum).unwrap(), datum);
    }

    #[test]
    fn maps_with_many_blocks_do_not_exhaust_the_stack() {
        let schema = map_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let mut datum = [0x02, 0x02, b'a', 0x00].repeat(100_000);
        datum.push(0x00);
        let sorted = sort_map_entries(&ctx, &schema, &datum).unwrap();
        assert_eq!(sorted.len(), 3 + 100_000 * 3 + 1);
    }

    #[test]
    fn logical_type_schemas_pass_through_their_underlying_encoding() {
        let cases: [(&str, Vec<u8>); 15] = [
            (r#"{"type":"int","logicalType":"date"}"#, vec![0x02]),
            (r#"{"type":"int","logicalType":"time-millis"}"#, vec![0x02]),
            (r#"{"type":"long","logicalType":"time-micros"}"#, vec![0x02]),
            (
                r#"{"type":"long","logicalType":"timestamp-millis"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"long","logicalType":"timestamp-micros"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"long","logicalType":"timestamp-nanos"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"long","logicalType":"local-timestamp-millis"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"long","logicalType":"local-timestamp-micros"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"long","logicalType":"local-timestamp-nanos"}"#,
                vec![0x02],
            ),
            (
                r#"{"type":"string","logicalType":"uuid"}"#,
                vec![0x02, b'a'],
            ),
            (
                r#"{"type":"bytes","logicalType":"big-decimal"}"#,
                vec![0x02, 0x05],
            ),
            (
                r#"{"type":"bytes","logicalType":"decimal","precision":4,"scale":2}"#,
                vec![0x02, 0x05],
            ),
            (
                r#"{"type":"fixed","name":"D","size":2,"logicalType":"decimal","precision":4,"scale":2}"#,
                vec![7; 2],
            ),
            (
                r#"{"type":"fixed","name":"Dur","size":12,"logicalType":"duration"}"#,
                vec![7; 12],
            ),
            (
                r#"{"type":"fixed","name":"U","size":16,"logicalType":"uuid"}"#,
                vec![7; 16],
            ),
        ];
        cases.iter().for_each(|(json, datum)| {
            let schema = Schema::parse_str(json).unwrap();
            let ctx = SchemaCtx::new(&schema).unwrap();
            assert_eq!(
                sort_map_entries(&ctx, &schema, datum).as_ref(),
                Ok(datum),
                "{json}"
            );
        });
    }
}
