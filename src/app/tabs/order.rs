//! Tab ordering maths: where a tab lands when another one moves.

/// Where tab `i` ends up when the tab at `from` moves to `to`.
pub(crate) fn moved(i: usize, from: usize, to: usize) -> usize {
    if i == from {
        to
    } else if from < to && i > from && i <= to {
        i - 1
    } else if to < from && i >= to && i < from {
        i + 1
    } else {
        i
    }
}

/// For `order` (new position k holds old tab `order[k]`): old index -> new
/// index. `None` unless `order` is a permutation of `0..len`.
pub(crate) fn inverse(order: &[usize], len: usize) -> Option<Vec<usize>> {
    if order.len() != len {
        return None;
    }
    let mut inv = vec![usize::MAX; len];
    for (new, &old) in order.iter().enumerate() {
        if old >= len || inv[old] != usize::MAX {
            return None;
        }
        inv[old] = new;
    }
    Some(inv)
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
