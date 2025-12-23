use std::collections::{HashMap, HashSet};

pub const RELEVANCE_THRESHOLD: f32 = 0.05;

pub struct TfIdf {
    pub vocab: HashSet<String>,
    pub idf: HashMap<String, f32>,
}

pub fn compute_tfidf(documents: &[&str]) -> TfIdf {
    let mut vocab: HashSet<String> = HashSet::new();
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    let num_docs = documents.len() as f32;

    for doc in documents {
        let words: HashSet<String> = doc.split_whitespace().map(|w| w.to_lowercase()).collect();
        for word in &words {
            *doc_freq.entry(word.clone()).or_insert(0) += 1;
        }
        vocab.extend(words);
    }

    let mut idf: HashMap<String, f32> = HashMap::new();
    for word in &vocab {
        let df = *doc_freq.get(word).unwrap_or(&0) as f32;
        idf.insert(word.clone(), (num_docs / (df + 1.0)).ln() + 1.0);
    }

    TfIdf { vocab, idf }
}

pub fn tf_vector(text: &str, tfidf: &TfIdf) -> Vec<f32> {
    let mut word_counts: HashMap<String, usize> = HashMap::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    let total_words = words.len() as f32;

    for word in words {
        *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }

    tfidf
        .vocab
        .iter()
        .map(|word| {
            let tf = *word_counts.get(word).unwrap_or(&0) as f32 / total_words;
            let idf = *tfidf.idf.get(word).unwrap_or(&1.0); // Default to 1.0 if not found (neutral weight)
            tf * idf // TF-IDF value
        })
        .collect()
}

pub fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm1 == 0.0 || norm2 == 0.0 {
        0.0
    } else {
        dot_product / (norm1 * norm2)
    }
}

pub fn build_term_graph(content: &str) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    let words: Vec<&str> = content.split_whitespace().collect();

    for i in 0..words.len().saturating_sub(1) {
        let w1 = words[i].to_lowercase();
        let w2 = words[i + 1].to_lowercase();
        graph.entry(w1.clone()).or_default().insert(w2.clone());
        graph.entry(w2).or_default().insert(w1);
    }

    graph
}

pub fn graph_similarity(
    query_graph: &HashMap<String, HashSet<String>>,
    doc_graph: &HashMap<String, HashSet<String>>,
) -> f32 {
    let query_terms: HashSet<_> = query_graph.keys().collect();
    let doc_terms: HashSet<_> = doc_graph.keys().collect();
    let intersection = query_terms.intersection(&doc_terms).count() as f32;
    let union = query_terms.union(&doc_terms).count() as f32;

    let term_similarity = if union == 0.0 {
        0.0
    } else {
        intersection / union
    };

    let mut edge_similarity_sum = 0.0;
    let mut shared_count = 0;
    let empty_set: HashSet<String> = HashSet::new();
    for term in query_terms.intersection(&doc_terms) {
        let query_edges = query_graph.get(*term).unwrap_or(&empty_set);
        let doc_edges = doc_graph.get(*term).unwrap_or(&empty_set);
        let edge_intersection = query_edges.intersection(doc_edges).count() as f32;
        let edge_union = query_edges.union(doc_edges).count() as f32;
        edge_similarity_sum += if edge_union == 0.0 {
            0.0
        } else {
            edge_intersection / edge_union
        };
        shared_count += 1;
    }

    let edge_similarity = if shared_count == 0 {
        0.0
    } else {
        edge_similarity_sum / shared_count as f32
    };

    0.5 * term_similarity + 0.5 * edge_similarity
}