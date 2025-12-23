use textdistance::str;

pub const RELEVANCE_THRESHOLD: f32 = 0.05;

pub fn cosine_similarity(text1: &str, text2: &str) -> f32 {
    str::cosine(text1, text2) as f32
}