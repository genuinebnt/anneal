//! anneal's content: tracks, stages and problems stored as folders under `content/`.
//!
//! [`Catalog::load`] reads everything and reports every problem it finds with the
//! content as an [`Issue`], so `anneal validate` can list them all at once.

pub mod bundle;
mod catalog;
pub mod course;
mod dsa;
pub mod model;
pub mod test_source;

pub use catalog::{Catalog, Issue, LoadError, Loaded, Problem, ProblemFiles, Track};
pub use dsa::{Approach, Company, CompanyGroup, DsaCatalog, DsaProblem, External, Extra, IDEA_DEMOS, IDEA_GLYPHS, Idea, IdeaCost, IdeaWhich, IdeaWith, Lesson, Listed, Page, Role, Technique, Variant};
pub use model::{Band, COMPANIES, Language, Mode, Perf, Rules, Section, Status, Tier};
