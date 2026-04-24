use crate::eval::runtime_value::RuntimeValue;

use super::{Error, MAX_RANGE_SIZE};

pub(super) fn generate_numeric_range(start: isize, end: isize, step: isize) -> Result<Vec<RuntimeValue>, Error> {
    if step == 0 {
        return Err(Error::Runtime("step for range must not be zero".to_string()));
    }

    let range_size = if (step > 0 && end >= start) || (step < 0 && end <= start) {
        let diff = (end as i128) - (start as i128);
        let step_i128 = step as i128;
        ((diff / step_i128).abs() + 1) as usize
    } else {
        0
    };

    if range_size > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            range_size, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(range_size);
    let mut current = start;

    if step > 0 {
        while current <= end {
            result.push(RuntimeValue::Number(current.into()));
            current += step;
        }
    } else {
        while current >= end {
            result.push(RuntimeValue::Number(current.into()));
            current += step;
        }
    }

    Ok(result)
}

pub(super) fn generate_char_range(
    start_char: char,
    end_char: char,
    step: Option<i32>,
) -> Result<Vec<RuntimeValue>, Error> {
    let step = step.unwrap_or(if start_char <= end_char { 1 } else { -1 });

    if step == 0 {
        return Err(Error::Runtime("step for range must not be zero".to_string()));
    }

    let range_size = if (step > 0 && end_char >= start_char) || (step < 0 && end_char <= start_char) {
        let diff = (end_char as i64) - (start_char as i64);
        let step_i64 = step as i64;
        ((diff / step_i64).abs() + 1) as usize
    } else {
        0
    };

    if range_size > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            range_size, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(range_size);
    let mut current = start_char as i32;
    let end = end_char as i32;

    if step > 0 {
        while current <= end {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(RuntimeValue::String(ch.to_string()));
            }
            current += step;
        }
    } else {
        while current >= end {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(RuntimeValue::String(ch.to_string()));
            }
            current += step;
        }
    }

    Ok(result)
}

pub(super) fn generate_multi_char_range(start: &str, end: &str) -> Result<Vec<RuntimeValue>, Error> {
    if start.len() != end.len() {
        return Err(Error::Runtime(
            "String range requires strings of equal length".to_string(),
        ));
    }

    let start_bytes = start.as_bytes();
    let end_bytes = end.as_bytes();

    let capacity_estimate = (end_bytes.iter().zip(start_bytes.iter()))
        .map(|(e, s)| (e.max(s) - e.min(s)) as usize)
        .try_fold(0usize, |acc, diff| {
            acc.checked_add(diff)
                .ok_or_else(|| Error::Runtime(format!("range size exceeds maximum allowed size of {}", MAX_RANGE_SIZE)))
        })?
        + 1;

    if capacity_estimate > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            capacity_estimate, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(capacity_estimate);
    let mut current = start_bytes.to_vec();

    loop {
        if let Ok(s) = String::from_utf8(current.clone()) {
            result.push(RuntimeValue::String(s));
        }

        if current.as_slice() == end_bytes {
            break;
        }

        let mut carry = true;
        for byte in current.iter_mut().rev() {
            if carry {
                if *byte < 255 {
                    *byte += 1;
                    carry = false;
                } else {
                    *byte = 0;
                }
            }
        }

        if carry || current.as_slice() > end_bytes {
            break;
        }
    }

    Ok(result)
}
