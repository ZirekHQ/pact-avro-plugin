use crate::avro::schema::SchemaCtx;
use apache_avro::schema::Schema;

type Rest<'b> = Result<&'b [u8], String>;
type Entry = (Vec<u8>, Vec<u8>);

/// Rewrites an Avro datum so every map block lists its entries in ascending key order.
pub fn sort_map_entries<'a>(
    ctx: &SchemaCtx<'a>,
    schema: &'a Schema,
    datum: &[u8],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(datum.len());
    rewrite(ctx, schema, datum, &mut out).map(|_| out)
}

fn read_long(input: &[u8]) -> Result<(i64, &[u8]), String> {
    input
        .iter()
        .take(10)
        .position(|byte| byte & 0x80 == 0)
        .map(|last| {
            let raw = input[..=last]
                .iter()
                .enumerate()
                .fold(0u64, |acc, (i, byte)| {
                    acc | u64::from(byte & 0x7f) << (7 * i)
                });
            (
                ((raw >> 1) as i64) ^ -((raw & 1) as i64),
                &input[last + 1..],
            )
        })
        .ok_or_else(|| "truncated variable-length integer".to_string())
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
        Schema::Fixed(fixed) => copy(input, fixed.size, out),
        Schema::Int | Schema::Long | Schema::Enum(_) => copy_long(input, out),
        Schema::Bytes | Schema::String => copy_sized(input, out),
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
    let (count, rest) = read_block_count(input)?;
    write_long(count as i64, out);
    match count {
        0 => Ok(rest),
        _ => (0..count)
            .try_fold(rest, |rest, _| rewrite(ctx, items, rest, out))
            .and_then(|rest| rewrite_array(ctx, items, rest, out)),
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
    let (count, rest) = read_block_count(input)?;
    match count {
        0 => Ok(rest),
        _ => (0..count)
            .try_fold(rest, |rest, _| read_entry(ctx, values, rest, entries))
            .and_then(|rest| read_entries(ctx, values, rest, entries)),
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
}
