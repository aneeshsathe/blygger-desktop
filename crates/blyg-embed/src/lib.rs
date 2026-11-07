//! blyg-embed: text embeddings for Nearby thoughts (the rail beside the
//! editor that finds your own posts, and the posts you hold, that say the
//! same thing as the paragraph you're writing).
//!
//! - [`Embedder`]: text in, one unit vector per text out. The app has three
//!   ways to fill it: [`StaticModel`] (a model2vec static-embedding model,
//!   run here in pure Rust), an OpenAI-compatible or Ollama endpoint the
//!   user configured (`blyg_ai::embeddings`), and none (the rail falls back
//!   to phrase matches in the full-text index).
//! - [`model`]: the static model: a token → vector table from a
//!   safetensors file, a BERT WordPiece tokenizer ([`tokenizer`]) read from
//!   the model's `tokenizer.json`, mean-pooled and normalised.
//! - [`download`]: the one model Burrow offers ([`download::POTION_BASE_8M`]),
//!   pinned by commit, size and sha256, fetched only when the user turns it
//!   on.
//! - [`chunk`]: a post's text as paragraph-sized pieces to embed.
//! - [`vector`]: cosine over unit vectors, top-k, and the f16 storage form.
//!
//! No C, no ONNX: it builds wherever the rest of Burrow does.

pub mod chunk;
pub mod download;
pub mod model;
pub mod testing;
pub mod tokenizer;
pub mod vector;

pub use model::StaticModel;

/// Why an embedding couldn't be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// The model's files are missing or unreadable.
    Model(String),
    /// An endpoint couldn't be reached or refused.
    Network(String),
    /// An endpoint answered with something else.
    Provider(String),
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedError::Model(m) => write!(f, "the embedding model: {m}"),
            EmbedError::Network(m) => write!(f, "couldn't reach the embeddings endpoint: {m}"),
            EmbedError::Provider(m) => write!(f, "the embeddings endpoint: {m}"),
        }
    }
}

impl std::error::Error for EmbedError {}

pub type Result<T> = std::result::Result<T, EmbedError>;

/// Text in, vectors out. Implementations are blocking and called off the
/// main thread; every vector they return is unit length ([`vector::normalize`]),
/// so cosine similarity is a dot product.
pub trait Embedder: Send + Sync {
    /// Names the model and its version: vectors are stored with it, and a
    /// different id means everything is embedded again.
    fn id(&self) -> String;
    /// One vector per text, in order, all the same length.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    /// How many texts one `embed` call should carry (an endpoint's batch).
    fn batch(&self) -> usize {
        64
    }
}
