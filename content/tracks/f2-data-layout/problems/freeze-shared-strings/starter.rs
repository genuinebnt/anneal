use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeriesId(pub u32);

/// A series' labels as handed to the query engine. They must outlive the `Head` (compaction replaces it).
pub type Labels = Vec<(String, String)>;

/// One time series in the head block.
pub struct Series {
    name: String,
    labels: Labels,
    samples: Vec<(i64, f64)>,
}

/// A TSDB head block: every series written since the last compaction.
pub struct Head {
    series: Vec<Series>,
}

impl Head {
    /// Room for `series` series and `symbols` distinct strings, reserved up front.
    pub fn with_capacity(series: usize, symbols: usize) -> Head {
        let _ = symbols;
        Head {
            series: Vec::with_capacity(series),
        }
    }

    /// Adds a series. `labels` arrive sorted by name, as Prometheus keeps them.
    pub fn add_series(&mut self, name: &str, labels: &[(&str, &str)]) -> SeriesId {
        let id = SeriesId(self.series.len() as u32);
        let labels = labels
            .iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.series.push(Series {
            name: name.to_string(),
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
            .binary_search_by(|(k, _)| k.as_str().cmp(key))
            .ok()
            .map(|i| labels[i].1.as_str())
    }

    pub fn samples(&self, id: SeriesId) -> &[(i64, f64)] {
        &self.series[id.0 as usize].samples
    }

    /// The labels of `id`, for the query engine to keep.
    pub fn labels_of(&self, id: SeriesId) -> Labels {
        self.series[id.0 as usize].labels.clone()
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
        let mut seen = HashSet::new();
        for s in &self.series {
            seen.insert(s.name.as_str());
            for (k, v) in &s.labels {
                seen.insert(k.as_str());
                seen.insert(v.as_str());
            }
        }
        seen.len()
    }
}
