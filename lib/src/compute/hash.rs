use arrow::array::ArrayData;
use blake3::{Hash, Hasher};

pub fn hash(arr: &ArrayData) -> Hash {
    let mut hasher = Hasher::new();

    hasher.update(&arr.len().to_le_bytes());
    hasher.update(&arr.offset().to_le_bytes());

    /// Update `hasher` using `update_rayon` OR `update` depending on buffer size
    macro_rules! update_size_dependent {
        ($name:ident, $update_expr:expr) => {
            if $name.len() > 1024_usize.pow(2) {
                hasher.update_rayon($update_expr);
            } else {
                hasher.update($update_expr);
            }
        };
    }

    for buf in arr.buffers() {
        update_size_dependent!(buf, &buf[..]);
    }

    // Hash null bitmap if it exists
    if let Some(null_buf) = arr.nulls() {
        update_size_dependent!(null_buf, null_buf.validity());
    }

    // Recursively hash children (e.g., ListArray, StructArray)
    for child in arr.child_data() {
        update_size_dependent!(child, hash(child).as_bytes());
    }

    hasher.finalize()
}
