use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeriesId(pub u32);

/// A series' labels as handed to the query engine. They must outlive the `Head` (compaction replaces it).
/// Frozen at creation, so a shared slice: one allocation holding the count and the pairs, cloned by a
/// reference count bump.
pub type Labels = Arc<[(Arc<str>, Arc<str>)]>;

/// One time series in the head block. `name` is shared by every series of the metric; `samples` is the
/// only part that grows, so it's the only `Vec`.
pub struct Series {
    name: Arc<str>,
    labels: Labels,
    samples: Vec<(i64, f64)>,
}

/// A TSDB head block: every series written since the last compaction.
pub struct Head {
    symbols: HashSet<Arc<str>>,
    series: Vec<Series>,
}

impl Head {
    /// Room for `series` series and `symbols` distinct strings, reserved up front.
    pub fn with_capacity(series: usize, symbols: usize) -> Head {
        Head {
            symbols: HashSet::with_capacity(symbols),
            series: Vec::with_capacity(series),
        }
    }

    /// The shared copy of `s`, made on first sight. `Arc<str>: Borrow<str>`, so the lookup takes a `&str`.
    fn intern(symbols: &mut HashSet<Arc<str>>, s: &str) -> Arc<str> {
        if let Some(sym) = symbols.get(s) {
            return Arc::clone(sym);
        }
        let sym: Arc<str> = Arc::from(s);
        symbols.insert(Arc::clone(&sym));
        sym
    }

    /// Adds a series. `labels` arrive sorted by name, as Prometheus keeps them.
    pub fn add_series(&mut self, name: &str, labels: &[(&str, &str)]) -> SeriesId {
        let id = SeriesId(self.series.len() as u32);
        let symbols = &mut self.symbols;
        let name = Head::intern(symbols, name);
        // A mapped slice iterator knows its length, so this collects straight into one allocation.
        let labels: Labels = labels
            .iter()
            .map(|&(k, v)| (Head::intern(symbols, k), Head::intern(symbols, v)))
            .collect();
        self.series.push(Series {
            name,
            labels,
            samples: Vec::new(),
        });
        id
    }

    pub fn append(&mut self, id: SeriesId, t: i64, v: f64) {
        self.series[id.0 as usize].samples.push((t, v));
    }

    pub fn name(&self, id: SeriesId) -> &str {
        &self.series[id.0 as usize].name
    }

    pub fn label(&self, id: SeriesId, key: &str) -> Option<&str> {
        let labels = &self.series[id.0 as usize].labels;
        labels
            .binary_search_by(|(k, _)| k[..].cmp(key))
            .ok()
            .map(|i| &labels[i].1[..])
    }

    pub fn samples(&self, id: SeriesId) -> &[(i64, f64)] {
        &self.series[id.0 as usize].samples
    }

    /// The labels of `id`, for the query engine to keep: a reference count bump, no copy.
    pub fn labels_of(&self, id: SeriesId) -> Labels {
        Arc::clone(&self.series[id.0 as usize].labels)
    }

    /// Every series with label `key` = `value`, in id order.
    pub fn select(&self, key: &str, value: &str) -> Vec<SeriesId> {
        (0..self.series.len() as u32)
            .map(SeriesId)
            .filter(|&id| self.label(id, key) == Some(value))
            .collect()
    }

    /// How many distinct strings (metric names, label names and values) the head holds.
    pub fn symbols(&self) -> usize {
        self.symbols.len()
    }
}
