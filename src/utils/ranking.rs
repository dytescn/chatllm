/// 排序权重配置。
#[derive(Debug, Clone, Copy)]
pub struct RankWeights {
    /// 语义相似度权重
    pub semantic: f64,
    /// 调用热度权重
    pub heat: f64,
}

impl Default for RankWeights {
    fn default() -> Self {
        Self {
            semantic: 0.85,
            heat: 0.15,
        }
    }
}

/// 候选集内 min-max 归一化：把 distance 映射到 [0,1] 的相似度。
///
/// distance 越小越相似，所以 similarity = 1 - distance/max_distance。
/// max_distance 为 0 时（所有候选距离相同）统一返回 1.0。
fn normalize_similarity(distance: f64, max_distance: f64) -> f64 {
    if max_distance <= 1e-12 {
        1.0
    } else {
        (1.0 - distance / max_distance).clamp(0.0, 1.0)
    }
}

/// 调用次数归一化到 [0,1]。
fn normalize_heat(call_count: i64, max_count: i64) -> f64 {
    if max_count <= 0 {
        0.0
    } else {
        (call_count as f64 / max_count as f64).clamp(0.0, 1.0)
    }
}

/// 对候选集计算最终分并降序排序，返回排序后的候选。
pub fn rank_candidates(mut candidates: Vec<Candidate>, weights: RankWeights) -> Vec<Candidate> {
    if candidates.is_empty() {
        return candidates;
    }

    let max_distance = candidates
        .iter()
        .map(|c| c.distance)
        .fold(0.0_f64, f64::max);
    let max_count = candidates.iter().map(|c| c.call_count).max().unwrap_or(0);

    // 预先算分，避免 sort_by 里重复计算
    let mut scored: Vec<(f64, Candidate)> = candidates
        .drain(..)
        .map(|c| {
            let sim = normalize_similarity(c.distance, max_distance);
            let heat = normalize_heat(c.call_count, max_count);
            let score = weights.semantic * sim + weights.heat * heat;
            (score, c)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored.into_iter().map(|(_, c)| c).collect()
}

