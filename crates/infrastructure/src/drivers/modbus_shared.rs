use domain::error::DomainError;
use domain::id::TagId;
use domain::tag::TagValue;
use std::collections::HashMap;
use tokio::time::{Duration, sleep, timeout};
use tokio_modbus::client::Context;
use tokio_modbus::prelude::{Reader, Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusArea {
    Holding,
    Input,
    Coil,
    DiscreteInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusEncoding {
    U16,
    I16,
    U32,
    U32Le,
    I32,
    I32Le,
    F32,
    F32Le,
    Bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModbusPoint {
    pub area: ModbusArea,
    pub address: u16,
    pub encoding: ModbusEncoding,
    pub scale: f64,
    pub offset: f64,
}

impl ModbusPoint {
    pub fn parse(source: &str) -> Result<Self, DomainError> {
        let parts: Vec<&str> = source.split(':').collect();
        if parts.len() < 3 || parts.len() > 6 {
            return Err(DomainError::ConfigurationError(format!(
                "invalid modbus source '{}', expected area:address:encoding[:word_order][:scale[:offset]]",
                source
            )));
        }

        let area = match parts[0].trim().to_ascii_lowercase().as_str() {
            "hr" | "holding" => ModbusArea::Holding,
            "ir" | "input" => ModbusArea::Input,
            "coil" => ModbusArea::Coil,
            "di" | "discrete" => ModbusArea::DiscreteInput,
            other => {
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported modbus area '{}'",
                    other
                )));
            }
        };

        let address = parts[1].trim().parse::<u16>().map_err(|_| {
            DomainError::ConfigurationError(format!("invalid modbus address '{}'", parts[1]))
        })?;

        let mut encoding = match parts[2].trim().to_ascii_lowercase().as_str() {
            "u16" | "uint16" => ModbusEncoding::U16,
            "i16" | "int16" => ModbusEncoding::I16,
            "u32" | "uint32" => ModbusEncoding::U32,
            "u32le" | "u32_le" | "u32-le" => ModbusEncoding::U32Le,
            "i32" | "int32" => ModbusEncoding::I32,
            "i32le" | "i32_le" | "i32-le" => ModbusEncoding::I32Le,
            "f32" | "float32" => ModbusEncoding::F32,
            "f32le" | "f32_le" | "f32-le" => ModbusEncoding::F32Le,
            "bool" => ModbusEncoding::Bool,
            other => {
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported modbus encoding '{}'",
                    other
                )));
            }
        };

        // Backward compatible formats:
        // - area:address:encoding[:scale[:offset]]
        // New format:
        // - area:address:encoding:word_order[:scale[:offset]]
        let mut idx = 3usize;
        if parts.len() > idx {
            let maybe_word_order = parts[idx].trim().to_ascii_lowercase();
            let has_word_order = matches!(
                maybe_word_order.as_str(),
                "high_first"
                    | "highfirst"
                    | "hf"
                    | "big"
                    | "be"
                    | "low_first"
                    | "lowfirst"
                    | "lf"
                    | "little"
                    | "le"
            );
            if has_word_order {
                encoding = apply_word_order(encoding, maybe_word_order.as_str())?;
                idx += 1;
            }
        }

        let scale = if parts.len() > idx {
            parts[idx].trim().parse::<f64>().map_err(|_| {
                DomainError::ConfigurationError(format!("invalid modbus scale '{}'", parts[idx]))
            })?
        } else {
            1.0
        };
        let offset = if parts.len() > idx + 1 {
            parts[idx + 1].trim().parse::<f64>().map_err(|_| {
                DomainError::ConfigurationError(format!("invalid modbus offset '{}'", parts[idx + 1]))
            })?
        } else {
            0.0
        };

        match area {
            ModbusArea::Holding | ModbusArea::Input => match encoding {
                ModbusEncoding::U16
                | ModbusEncoding::I16
                | ModbusEncoding::U32
                | ModbusEncoding::U32Le
                | ModbusEncoding::I32
                | ModbusEncoding::I32Le
                | ModbusEncoding::F32 => {}
                | ModbusEncoding::F32Le => {}
                _ => {
                    return Err(DomainError::ConfigurationError(
                        "register areas only support u16/i16/u32/i32/f32 encoding".to_string(),
                    ));
                }
            },
            ModbusArea::Coil | ModbusArea::DiscreteInput => {
                if encoding != ModbusEncoding::Bool {
                    return Err(DomainError::ConfigurationError(
                        "coil/discrete areas only support bool encoding".to_string(),
                    ));
                }
                if scale != 1.0 || offset != 0.0 {
                    return Err(DomainError::ConfigurationError(
                        "coil/discrete areas do not support scale/offset".to_string(),
                    ));
                }
            }
        }
        if (matches!(
            encoding,
            ModbusEncoding::U16
                | ModbusEncoding::I16
                | ModbusEncoding::U32
                | ModbusEncoding::I32
                | ModbusEncoding::F32
        ))
            && scale == 0.0
        {
            return Err(DomainError::ConfigurationError(
                "modbus scale cannot be 0".to_string(),
            ));
        }

        Ok(Self {
            area,
            address,
            encoding,
            scale,
            offset,
        })
    }

    pub fn width(&self) -> u16 {
        match self.encoding {
            ModbusEncoding::U32
            | ModbusEncoding::U32Le
            | ModbusEncoding::I32
            | ModbusEncoding::I32Le
            | ModbusEncoding::F32
            | ModbusEncoding::F32Le => 2,
            _ => 1,
        }
    }
}

fn apply_word_order(
    encoding: ModbusEncoding,
    word_order: &str,
) -> Result<ModbusEncoding, DomainError> {
    let low_first = matches!(
        word_order,
        "low_first" | "lowfirst" | "lf" | "little" | "le"
    );
    let high_first = matches!(
        word_order,
        "high_first" | "highfirst" | "hf" | "big" | "be"
    );
    if !low_first && !high_first {
        return Err(DomainError::ConfigurationError(format!(
            "unsupported modbus word_order '{}'",
            word_order
        )));
    }

    let normalized = match (encoding, low_first) {
        (ModbusEncoding::U32, true) => ModbusEncoding::U32Le,
        (ModbusEncoding::U32Le, false) => ModbusEncoding::U32,
        (ModbusEncoding::I32, true) => ModbusEncoding::I32Le,
        (ModbusEncoding::I32Le, false) => ModbusEncoding::I32,
        (ModbusEncoding::F32, true) => ModbusEncoding::F32Le,
        (ModbusEncoding::F32Le, false) => ModbusEncoding::F32,
        (other, _) => other,
    };

    if matches!(
        normalized,
        ModbusEncoding::U16 | ModbusEncoding::I16 | ModbusEncoding::Bool
    ) {
        return Err(DomainError::ConfigurationError(
            "word_order only applies to 32-bit encodings (u32/i32/f32)".to_string(),
        ));
    }
    Ok(normalized)
}

#[derive(Debug, Clone, Copy)]
pub struct ModbusBatchPolicy {
    pub max_batch_registers: u16,
    pub max_batch_bits: u16,
    pub max_register_gap: u16,
    pub max_bit_gap: u16,
}

impl Default for ModbusBatchPolicy {
    fn default() -> Self {
        Self {
            max_batch_registers: 120,
            max_batch_bits: 2000,
            max_register_gap: 0,
            max_bit_gap: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModbusRequestPolicy {
    pub timeout_ms: u64,
    pub retries: u8,
    pub retry_backoff_ms: u64,
    pub retry_backoff_strategy: RetryBackoffStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum RetryBackoffStrategy {
    Fixed,
    Exponential { max_ms: u64 },
}

impl Default for ModbusRequestPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 1500,
            retries: 1,
            retry_backoff_ms: 100,
            retry_backoff_strategy: RetryBackoffStrategy::Fixed,
        }
    }
}

#[derive(Debug, Clone)]
struct ReadBatch {
    area: ModbusArea,
    start: u16,
    len: u16,
    tags: Vec<(TagId, ModbusPoint)>,
}

pub fn build_point_map(
    tag_map: &HashMap<String, String>,
) -> Result<HashMap<TagId, ModbusPoint>, DomainError> {
    let mut points = HashMap::new();
    for (tag_id, source) in tag_map {
        let p = ModbusPoint::parse(source)?;
        points.insert(TagId::new(tag_id), p);
    }
    Ok(points)
}

pub async fn poll_points_batched(
    ctx: &mut Context,
    points: &HashMap<TagId, ModbusPoint>,
    batch: ModbusBatchPolicy,
    req: ModbusRequestPolicy,
) -> Result<Vec<(TagId, Result<TagValue, DomainError>)>, DomainError> {
    let mut out: Vec<(TagId, Result<TagValue, DomainError>)> = Vec::with_capacity(points.len());
    let plan = build_read_plan(points, batch);

    for b in plan {
        match read_batch_with_retry(ctx, &b, req).await {
            Ok(data) => {
                for (tag_id, point) in &b.tags {
                    let offset = point.address.saturating_sub(b.start) as usize;
                    let value = decode_value(point, &data[offset..]);
                    out.push((tag_id.clone(), value));
                }
            }
            Err(e) => {
                let msg = e.to_string();
                for (tag_id, _) in &b.tags {
                    out.push((tag_id.clone(), Err(DomainError::DriverError(msg.clone()))));
                }
            }
        }
    }
    Ok(out)
}

pub async fn write_point(
    ctx: &mut Context,
    point: &ModbusPoint,
    value: TagValue,
    req: ModbusRequestPolicy,
) -> Result<(), DomainError> {
    match point.area {
        ModbusArea::Holding => {
            let normalized = apply_inverse_scaling(point, value)?;
            let regs = encode_register_value(point.encoding, normalized)?;
            if regs.len() == 1 {
                write_single_register_with_retry(ctx, point.address, regs[0], req).await
            } else {
                write_multi_register_with_retry(ctx, point.address, regs, req).await
            }
        }
        ModbusArea::Coil => {
            let raw = match value {
                TagValue::Boolean(v) => v,
                _ => return Err(DomainError::DriverError("coil write expects boolean".to_string())),
            };
            write_single_coil_with_retry(ctx, point.address, raw, req).await
        }
        ModbusArea::Input | ModbusArea::DiscreteInput => Err(DomainError::DriverError(
            "cannot write to read-only modbus input area".to_string(),
        )),
    }
}

fn build_read_plan(
    points: &HashMap<TagId, ModbusPoint>,
    policy: ModbusBatchPolicy,
) -> Vec<ReadBatch> {
    let mut entries: Vec<(TagId, ModbusPoint)> =
        points.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    entries.sort_by_key(|(_, p)| (area_order(p.area), p.address));

    let mut batches = Vec::new();
    for area in [
        ModbusArea::Holding,
        ModbusArea::Input,
        ModbusArea::Coil,
        ModbusArea::DiscreteInput,
    ] {
        let max_len = match area {
            ModbusArea::Holding | ModbusArea::Input => policy.max_batch_registers.max(1),
            ModbusArea::Coil | ModbusArea::DiscreteInput => policy.max_batch_bits.max(1),
        };
        let max_gap = match area {
            ModbusArea::Holding | ModbusArea::Input => policy.max_register_gap,
            ModbusArea::Coil | ModbusArea::DiscreteInput => policy.max_bit_gap,
        };
        let area_entries: Vec<(TagId, ModbusPoint)> = entries
            .iter()
            .filter(|(_, p)| p.area == area)
            .cloned()
            .collect();
        if area_entries.is_empty() {
            continue;
        }

        let mut cur_start = area_entries[0].1.address;
        let mut cur_end = cur_start + area_entries[0].1.width();
        let mut cur_tags = vec![area_entries[0].clone()];

        for entry in area_entries.into_iter().skip(1) {
            let p = &entry.1;
            let next_end = p.address.saturating_add(p.width());
            let proposed_len = next_end.saturating_sub(cur_start);
            let allowed_end = cur_end.saturating_add(max_gap);
            if p.address <= allowed_end && proposed_len <= max_len {
                cur_end = cur_end.max(next_end);
                cur_tags.push(entry);
            } else {
                batches.push(ReadBatch {
                    area,
                    start: cur_start,
                    len: cur_end.saturating_sub(cur_start),
                    tags: cur_tags,
                });
                cur_start = p.address;
                cur_end = p.address.saturating_add(p.width());
                cur_tags = vec![entry];
            }
        }
        batches.push(ReadBatch {
            area,
            start: cur_start,
            len: cur_end.saturating_sub(cur_start),
            tags: cur_tags,
        });
    }
    batches
}

async fn read_batch_with_retry(
    ctx: &mut Context,
    batch: &ReadBatch,
    req: ModbusRequestPolicy,
) -> Result<Vec<u16>, DomainError> {
    let attempts = req.retries.saturating_add(1);
    let timeout_dur = Duration::from_millis(req.timeout_ms.max(1));
    for attempt in 0..attempts {
        let read_future = async {
            match batch.area {
                ModbusArea::Holding => {
                    let data = ctx.read_holding_registers(batch.start, batch.len).await;
                    flatten_read_result(data, "holding")
                }
                ModbusArea::Input => {
                    let data = ctx.read_input_registers(batch.start, batch.len).await;
                    flatten_read_result(data, "input")
                }
                ModbusArea::Coil => {
                    let data = ctx.read_coils(batch.start, batch.len).await;
                    let bits = flatten_read_result(data, "coil")?;
                    Ok(bits
                        .into_iter()
                        .map(|b| if b != 0 { 1u16 } else { 0u16 })
                        .collect())
                }
                ModbusArea::DiscreteInput => {
                    let data = ctx.read_discrete_inputs(batch.start, batch.len).await;
                    let bits = flatten_read_result(data, "discrete")?;
                    Ok(bits
                        .into_iter()
                        .map(|b| if b != 0 { 1u16 } else { 0u16 })
                        .collect())
                }
            }
        };

        match timeout(timeout_dur, read_future).await {
            Ok(Ok(vals)) => return Ok(vals),
            Ok(Err(e)) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(e);
                }
            }
            Err(_) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(DomainError::DriverError(format!(
                        "modbus read timeout after {} ms",
                        req.timeout_ms
                    )));
                }
            }
        }
    }
    Err(DomainError::DriverError("unreachable read retry state".to_string()))
}

fn flatten_read_result<T: Into<u16> + Copy>(
    res: Result<Result<Vec<T>, tokio_modbus::Exception>, tokio_modbus::Error>,
    area_name: &str,
) -> Result<Vec<u16>, DomainError> {
    let data = res
        .map_err(|e| DomainError::DriverError(format!("modbus read {} failed: {}", area_name, e)))?
        .map_err(|e| {
            DomainError::DriverError(format!("modbus {} exception: {}", area_name, e))
        })?;
    Ok(data.into_iter().map(|v| v.into()).collect())
}

fn decode_value(point: &ModbusPoint, raw: &[u16]) -> Result<TagValue, DomainError> {
    let base = match point.encoding {
        ModbusEncoding::Bool => Ok(TagValue::Boolean(raw.first().copied().unwrap_or(0) != 0)),
        ModbusEncoding::U16 => Ok(TagValue::Integer(raw.first().copied().unwrap_or(0) as i64)),
        ModbusEncoding::I16 => {
            let v = raw.first().copied().unwrap_or(0) as i16;
            Ok(TagValue::Integer(v as i64))
        }
        ModbusEncoding::U32 => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("u32 decode requires 2 registers".to_string()));
            }
            let v = ((raw[0] as u32) << 16) | raw[1] as u32;
            Ok(TagValue::Integer(v as i64))
        }
        ModbusEncoding::U32Le => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("u32 decode requires 2 registers".to_string()));
            }
            let v = ((raw[1] as u32) << 16) | raw[0] as u32;
            Ok(TagValue::Integer(v as i64))
        }
        ModbusEncoding::I32 => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("i32 decode requires 2 registers".to_string()));
            }
            let bits = ((raw[0] as u32) << 16) | raw[1] as u32;
            let v = bits as i32;
            Ok(TagValue::Integer(v as i64))
        }
        ModbusEncoding::I32Le => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("i32 decode requires 2 registers".to_string()));
            }
            let bits = ((raw[1] as u32) << 16) | raw[0] as u32;
            let v = bits as i32;
            Ok(TagValue::Integer(v as i64))
        }
        ModbusEncoding::F32 => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("f32 decode requires 2 registers".to_string()));
            }
            let bits = ((raw[0] as u32) << 16) | raw[1] as u32;
            Ok(TagValue::Float(f32::from_bits(bits) as f64))
        }
        ModbusEncoding::F32Le => {
            if raw.len() < 2 {
                return Err(DomainError::DriverError("f32 decode requires 2 registers".to_string()));
            }
            let bits = ((raw[1] as u32) << 16) | raw[0] as u32;
            Ok(TagValue::Float(f32::from_bits(bits) as f64))
        }
    }?;
    apply_forward_scaling(point, base)
}

fn encode_register_value(encoding: ModbusEncoding, value: TagValue) -> Result<Vec<u16>, DomainError> {
    fn as_integral_i64(value: TagValue) -> Option<i64> {
        match value {
            TagValue::Integer(v) => Some(v),
            TagValue::Float(v) => {
                if v.is_finite() && v.fract() == 0.0 {
                    Some(v as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    match encoding {
        ModbusEncoding::U16 => match as_integral_i64(value) {
            Some(v) if (0..=u16::MAX as i64).contains(&v) => Ok(vec![v as u16]),
            _ => Err(DomainError::DriverError(
                "u16 write expects integer 0..65535".to_string(),
            )),
        },
        ModbusEncoding::I16 => match as_integral_i64(value) {
            Some(v) if (i16::MIN as i64..=i16::MAX as i64).contains(&v) => {
                Ok(vec![v as i16 as u16])
            }
            _ => Err(DomainError::DriverError(
                "i16 write expects integer -32768..32767".to_string(),
            )),
        },
        ModbusEncoding::U32 => match as_integral_i64(value) {
            Some(v) if (0..=u32::MAX as i64).contains(&v) => {
                let n = v as u32;
                Ok(vec![(n >> 16) as u16, (n & 0xFFFF) as u16])
            }
            _ => Err(DomainError::DriverError(
                "u32 write expects integer 0..4294967295".to_string(),
            )),
        },
        ModbusEncoding::U32Le => match as_integral_i64(value) {
            Some(v) if (0..=u32::MAX as i64).contains(&v) => {
                let n = v as u32;
                Ok(vec![(n & 0xFFFF) as u16, (n >> 16) as u16])
            }
            _ => Err(DomainError::DriverError(
                "u32 write expects integer 0..4294967295".to_string(),
            )),
        },
        ModbusEncoding::I32 => match as_integral_i64(value) {
            Some(v) if (i32::MIN as i64..=i32::MAX as i64).contains(&v) => {
                let n = v as i32 as u32;
                Ok(vec![(n >> 16) as u16, (n & 0xFFFF) as u16])
            }
            _ => Err(DomainError::DriverError(
                "i32 write expects integer -2147483648..2147483647".to_string(),
            )),
        },
        ModbusEncoding::I32Le => match as_integral_i64(value) {
            Some(v) if (i32::MIN as i64..=i32::MAX as i64).contains(&v) => {
                let n = v as i32 as u32;
                Ok(vec![(n & 0xFFFF) as u16, (n >> 16) as u16])
            }
            _ => Err(DomainError::DriverError(
                "i32 write expects integer -2147483648..2147483647".to_string(),
            )),
        },
        ModbusEncoding::F32 => {
            let f = match value {
                TagValue::Float(v) => v as f32,
                TagValue::Integer(v) => v as f32,
                _ => {
                    return Err(DomainError::DriverError(
                        "f32 write expects float/integer".to_string(),
                    ));
                }
            };
            let bits = f.to_bits();
            Ok(vec![(bits >> 16) as u16, (bits & 0xFFFF) as u16])
        }
        ModbusEncoding::F32Le => {
            let f = match value {
                TagValue::Float(v) => v as f32,
                TagValue::Integer(v) => v as f32,
                _ => {
                    return Err(DomainError::DriverError(
                        "f32 write expects float/integer".to_string(),
                    ));
                }
            };
            let bits = f.to_bits();
            Ok(vec![(bits & 0xFFFF) as u16, (bits >> 16) as u16])
        }
        ModbusEncoding::Bool => Err(DomainError::DriverError(
            "bool encoding is not a register write".to_string(),
        )),
    }
}

fn apply_forward_scaling(point: &ModbusPoint, value: TagValue) -> Result<TagValue, DomainError> {
    if point.scale == 1.0 && point.offset == 0.0 {
        return Ok(value);
    }
    match value {
        TagValue::Integer(v) => Ok(TagValue::Float((v as f64) * point.scale + point.offset)),
        TagValue::Float(v) => Ok(TagValue::Float(v * point.scale + point.offset)),
        _ => Err(DomainError::DriverError(
            "scaling requires numeric modbus value".to_string(),
        )),
    }
}

fn apply_inverse_scaling(point: &ModbusPoint, value: TagValue) -> Result<TagValue, DomainError> {
    if point.scale == 1.0 && point.offset == 0.0 {
        return Ok(value);
    }
    let numeric = match value {
        TagValue::Integer(v) => v as f64,
        TagValue::Float(v) => v,
        _ => {
            return Err(DomainError::DriverError(
                "scaled modbus write expects numeric value".to_string(),
            ));
        }
    };
    let raw = (numeric - point.offset) / point.scale;
    Ok(TagValue::Float(raw))
}

async fn write_single_register_with_retry(
    ctx: &mut Context,
    address: u16,
    value: u16,
    req: ModbusRequestPolicy,
) -> Result<(), DomainError> {
    let attempts = req.retries.saturating_add(1);
    let timeout_dur = Duration::from_millis(req.timeout_ms.max(1));
    for attempt in 0..attempts {
        let fut = async {
            ctx.write_single_register(address, value)
                .await
                .map_err(|e| DomainError::DriverError(format!("modbus write register failed: {}", e)))?
                .map_err(|e| DomainError::DriverError(format!("modbus write exception: {}", e)))
        };
        match timeout(timeout_dur, fut).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(e);
                }
            }
            Err(_) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(DomainError::DriverError(format!(
                        "modbus write timeout after {} ms",
                        req.timeout_ms
                    )));
                }
            }
        }
    }
    Err(DomainError::DriverError(
        "unreachable write retry state".to_string(),
    ))
}

async fn write_multi_register_with_retry(
    ctx: &mut Context,
    address: u16,
    values: Vec<u16>,
    req: ModbusRequestPolicy,
) -> Result<(), DomainError> {
    let attempts = req.retries.saturating_add(1);
    let timeout_dur = Duration::from_millis(req.timeout_ms.max(1));
    for attempt in 0..attempts {
        let fut = async {
            ctx.write_multiple_registers(address, &values)
                .await
                .map_err(|e| {
                    DomainError::DriverError(format!("modbus write multiple failed: {}", e))
                })?
                .map_err(|e| DomainError::DriverError(format!("modbus write exception: {}", e)))
        };
        match timeout(timeout_dur, fut).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(e);
                }
            }
            Err(_) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(DomainError::DriverError(format!(
                        "modbus write timeout after {} ms",
                        req.timeout_ms
                    )));
                }
            }
        }
    }
    Err(DomainError::DriverError(
        "unreachable write retry state".to_string(),
    ))
}

async fn write_single_coil_with_retry(
    ctx: &mut Context,
    address: u16,
    value: bool,
    req: ModbusRequestPolicy,
) -> Result<(), DomainError> {
    let attempts = req.retries.saturating_add(1);
    let timeout_dur = Duration::from_millis(req.timeout_ms.max(1));
    for attempt in 0..attempts {
        let fut = async {
            ctx.write_single_coil(address, value)
                .await
                .map_err(|e| DomainError::DriverError(format!("modbus write coil failed: {}", e)))?
                .map_err(|e| DomainError::DriverError(format!("modbus write exception: {}", e)))
        };
        match timeout(timeout_dur, fut).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(e);
                }
            }
            Err(_) => {
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(backoff_delay_ms(req, attempt))).await;
                } else {
                    return Err(DomainError::DriverError(format!(
                        "modbus write timeout after {} ms",
                        req.timeout_ms
                    )));
                }
            }
        }
    }
    Err(DomainError::DriverError(
        "unreachable write retry state".to_string(),
    ))
}

fn backoff_delay_ms(req: ModbusRequestPolicy, attempt: u8) -> u64 {
    let base = req.retry_backoff_ms.max(1);
    match req.retry_backoff_strategy {
        RetryBackoffStrategy::Fixed => base,
        RetryBackoffStrategy::Exponential { max_ms } => {
            let shift = (attempt as u32).min(20);
            let mult = 1u64 << shift;
            base.saturating_mul(mult).min(max_ms.max(base))
        }
    }
}

fn area_order(area: ModbusArea) -> u8 {
    match area {
        ModbusArea::Holding => 0,
        ModbusArea::Input => 1,
        ModbusArea::Coil => 2,
        ModbusArea::DiscreteInput => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_modbus_point_hr_u16() {
        let p = ModbusPoint::parse("hr:10:u16").unwrap();
        assert_eq!(p.area, ModbusArea::Holding);
        assert_eq!(p.address, 10);
        assert_eq!(p.encoding, ModbusEncoding::U16);
        assert_eq!(p.scale, 1.0);
        assert_eq!(p.offset, 0.0);
    }

    #[test]
    fn test_parse_modbus_point_coil_bool() {
        let p = ModbusPoint::parse("coil:7:bool").unwrap();
        assert_eq!(p.area, ModbusArea::Coil);
        assert_eq!(p.address, 7);
        assert_eq!(p.encoding, ModbusEncoding::Bool);
        assert_eq!(p.scale, 1.0);
        assert_eq!(p.offset, 0.0);
    }

    #[test]
    fn test_parse_modbus_point_rejects_invalid_combo() {
        let err = ModbusPoint::parse("hr:1:bool").expect_err("must reject hr bool");
        match err {
            DomainError::ConfigurationError(msg) => {
                assert!(msg.contains("register areas only support"))
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_parse_modbus_point_supports_i16_u32_f32() {
        assert!(ModbusPoint::parse("hr:1:i16").is_ok());
        assert!(ModbusPoint::parse("hr:1:u32").is_ok());
        assert!(ModbusPoint::parse("hr:1:uint32").is_ok());
        assert!(ModbusPoint::parse("hr:1:u32le").is_ok());
        assert!(ModbusPoint::parse("hr:1:i32").is_ok());
        assert!(ModbusPoint::parse("hr:1:int32").is_ok());
        assert!(ModbusPoint::parse("hr:1:i32le").is_ok());
        assert!(ModbusPoint::parse("hr:1:f32").is_ok());
        assert!(ModbusPoint::parse("hr:1:float32").is_ok());
        assert!(ModbusPoint::parse("hr:1:f32le").is_ok());
    }

    #[test]
    fn test_parse_modbus_point_word_order_high_low() {
        let p_hf = ModbusPoint::parse("hr:8:u32:high_first").unwrap();
        assert_eq!(p_hf.encoding, ModbusEncoding::U32);

        let p_lf = ModbusPoint::parse("hr:8:u32:low_first").unwrap();
        assert_eq!(p_lf.encoding, ModbusEncoding::U32Le);

        let p_i32_lf = ModbusPoint::parse("hr:9:i32:low_first").unwrap();
        assert_eq!(p_i32_lf.encoding, ModbusEncoding::I32Le);

        let p_f32_lf = ModbusPoint::parse("hr:10:f32:lf").unwrap();
        assert_eq!(p_f32_lf.encoding, ModbusEncoding::F32Le);
    }

    #[test]
    fn test_parse_modbus_point_word_order_with_scale_offset() {
        let p = ModbusPoint::parse("hr:8:u32:high_first:0.1:0").unwrap();
        assert_eq!(p.encoding, ModbusEncoding::U32);
        assert!((p.scale - 0.1).abs() < 1e-9);
        assert!((p.offset - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_modbus_point_rejects_word_order_for_u16() {
        let err = ModbusPoint::parse("hr:1:u16:low_first").expect_err("must reject word order on u16");
        match err {
            DomainError::ConfigurationError(msg) => assert!(msg.contains("word_order only applies")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_parse_modbus_point_with_scale_and_offset() {
        let p = ModbusPoint::parse("hr:100:u16:0.1:-5").unwrap();
        assert_eq!(p.area, ModbusArea::Holding);
        assert_eq!(p.address, 100);
        assert_eq!(p.encoding, ModbusEncoding::U16);
        assert!((p.scale - 0.1).abs() < 1e-9);
        assert!((p.offset + 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_modbus_point_rejects_scale_on_bool() {
        let err = ModbusPoint::parse("coil:1:bool:2.0").expect_err("must reject bool scaling");
        match err {
            DomainError::ConfigurationError(msg) => assert!(msg.contains("do not support scale/offset")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_plan_batches_groups_contiguous_registers() {
        let mut points = HashMap::new();
        points.insert(TagId::new("t1"), ModbusPoint::parse("hr:0:u16").unwrap());
        points.insert(TagId::new("t2"), ModbusPoint::parse("hr:1:u16").unwrap());
        points.insert(TagId::new("t3"), ModbusPoint::parse("hr:10:u16").unwrap());
        let plan = build_read_plan(
            &points,
            ModbusBatchPolicy {
                max_batch_registers: 5,
                max_batch_bits: 2000,
                max_register_gap: 0,
                max_bit_gap: 0,
            },
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].start, 0);
        assert_eq!(plan[0].len, 2);
        assert_eq!(plan[1].start, 10);
    }

    #[test]
    fn test_plan_batches_allows_small_register_gap_for_rtu_efficiency() {
        let mut points = HashMap::new();
        points.insert(TagId::new("t1"), ModbusPoint::parse("hr:0:u16").unwrap());
        points.insert(TagId::new("t2"), ModbusPoint::parse("hr:3:u16").unwrap());
        let plan = build_read_plan(
            &points,
            ModbusBatchPolicy {
                max_batch_registers: 10,
                max_batch_bits: 2000,
                max_register_gap: 2,
                max_bit_gap: 0,
            },
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].start, 0);
        assert_eq!(plan[0].len, 4);
    }

    #[test]
    fn test_plan_batches_does_not_cross_large_register_gap() {
        let mut points = HashMap::new();
        points.insert(TagId::new("t1"), ModbusPoint::parse("hr:0:u16").unwrap());
        points.insert(TagId::new("t2"), ModbusPoint::parse("hr:4:u16").unwrap());
        let plan = build_read_plan(
            &points,
            ModbusBatchPolicy {
                max_batch_registers: 10,
                max_batch_bits: 2000,
                max_register_gap: 2,
                max_bit_gap: 0,
            },
        );
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn test_decode_encode_roundtrip_f32() {
        let p = ModbusPoint::parse("hr:0:f32").unwrap();
        let regs = encode_register_value(p.encoding, TagValue::Float(12.5)).unwrap();
        let v = decode_value(&p, &regs).unwrap();
        match v {
            TagValue::Float(x) => assert!((x - 12.5).abs() < 1e-4),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn test_decode_applies_scale_and_offset() {
        let p = ModbusPoint::parse("hr:0:u16:0.5:10").unwrap();
        let v = decode_value(&p, &[20]).unwrap();
        match v {
            TagValue::Float(x) => assert!((x - 20.0).abs() < 1e-9),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn test_inverse_scaling_for_write() {
        let p = ModbusPoint::parse("hr:0:u16:0.5:10").unwrap();
        let raw = apply_inverse_scaling(&p, TagValue::Float(20.0)).unwrap();
        let regs = encode_register_value(p.encoding, raw).unwrap();
        assert_eq!(regs, vec![20]);
    }

    #[test]
    fn test_decode_encode_roundtrip_u32_le() {
        let p = ModbusPoint::parse("hr:0:u32le").unwrap();
        let regs = encode_register_value(p.encoding, TagValue::Integer(0x11223344)).unwrap();
        assert_eq!(regs, vec![0x3344, 0x1122]);
        let v = decode_value(&p, &regs).unwrap();
        assert_eq!(v, TagValue::Integer(0x11223344));
    }

    #[test]
    fn test_decode_encode_roundtrip_i32() {
        let p = ModbusPoint::parse("hr:0:i32").unwrap();
        let regs = encode_register_value(p.encoding, TagValue::Integer(-123456)).unwrap();
        let v = decode_value(&p, &regs).unwrap();
        assert_eq!(v, TagValue::Integer(-123456));
    }

    #[test]
    fn test_decode_encode_roundtrip_i32_le() {
        let p = ModbusPoint::parse("hr:0:i32le").unwrap();
        let regs = encode_register_value(p.encoding, TagValue::Integer(-123456)).unwrap();
        let v = decode_value(&p, &regs).unwrap();
        assert_eq!(v, TagValue::Integer(-123456));
    }

    #[test]
    fn test_decode_encode_roundtrip_f32_le() {
        let p = ModbusPoint::parse("hr:0:f32le").unwrap();
        let regs = encode_register_value(p.encoding, TagValue::Float(12.5)).unwrap();
        let v = decode_value(&p, &regs).unwrap();
        match v {
            TagValue::Float(x) => assert!((x - 12.5).abs() < 1e-4),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn test_encode_u16_accepts_integral_float() {
        let regs = encode_register_value(ModbusEncoding::U16, TagValue::Float(123.0))
            .expect("integral float should be accepted for register writes");
        assert_eq!(regs, vec![123]);
    }

    #[test]
    fn test_encode_u16_rejects_fractional_float() {
        let err = encode_register_value(ModbusEncoding::U16, TagValue::Float(123.5))
            .expect_err("fractional float must be rejected for u16");
        match err {
            DomainError::DriverError(msg) => assert!(msg.contains("u16 write expects")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_backoff_delay_fixed() {
        let req = ModbusRequestPolicy {
            retry_backoff_ms: 50,
            retry_backoff_strategy: RetryBackoffStrategy::Fixed,
            ..Default::default()
        };
        assert_eq!(backoff_delay_ms(req, 0), 50);
        assert_eq!(backoff_delay_ms(req, 3), 50);
    }

    #[test]
    fn test_backoff_delay_exponential_capped() {
        let req = ModbusRequestPolicy {
            retry_backoff_ms: 25,
            retry_backoff_strategy: RetryBackoffStrategy::Exponential { max_ms: 100 },
            ..Default::default()
        };
        assert_eq!(backoff_delay_ms(req, 0), 25);
        assert_eq!(backoff_delay_ms(req, 1), 50);
        assert_eq!(backoff_delay_ms(req, 2), 100);
        assert_eq!(backoff_delay_ms(req, 3), 100);
    }
}
