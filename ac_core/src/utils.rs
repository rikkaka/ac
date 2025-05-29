/// 将f64阶段到小数点后第digits位
pub fn truncate_f64(x: f64, digits: u32) -> f64 {
    let factor = 10f64.powi(digits as i32);
    (x * factor).trunc() / factor
}

pub fn get_side_size_from_raw_size(raw_size: f64) -> (bool, f64) {
    if raw_size >= 0. {
        (true, raw_size.abs())
    } else {
        (false, raw_size.abs())
    }
}