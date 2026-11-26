pub enum SmallVec4<T> {
    Inline { items: [Option<T>; 4], len: usize },
    Heap(Vec<T>),
}

impl<T> SmallVec4<T> {
    pub fn new() -> Self {
        SmallVec4::Inline {
            items: [None, None, None, None],
            len: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        match self {
            SmallVec4::Inline { items, len } if *len < 4 => {
                items[*len] = Some(value);
                *len += 1;
            }
            SmallVec4::Inline { items, .. } => {
                let mut heap: Vec<T> = Vec::with_capacity(8);
                heap.extend(items.iter_mut().filter_map(Option::take));
                heap.push(value);
                *self = SmallVec4::Heap(heap);
            }
            SmallVec4::Heap(heap) => heap.push(value),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            SmallVec4::Inline { len, .. } => *len,
            SmallVec4::Heap(heap) => heap.len(),
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        match self {
            SmallVec4::Inline { items, len } if i < *len => items[i].as_ref(),
            SmallVec4::Inline { .. } => None,
            SmallVec4::Heap(heap) => heap.get(i),
        }
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, SmallVec4::Inline { .. })
    }
}

impl<T> Default for SmallVec4<T> {
    fn default() -> Self {
        Self::new()
    }
}
