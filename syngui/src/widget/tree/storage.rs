use hashbrown::HashMap;
use super::{ElementId, ElementNode};

pub(crate) struct ElementStorage {
    nodes: Vec<Option<ElementNode>>,
    id_to_idx: HashMap<ElementId, u32>,
    free_list: Vec<u32>,
    active: usize,
}

impl ElementStorage {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            id_to_idx: HashMap::new(),
            free_list: Vec::new(),
            active: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize { self.active }

    #[allow(dead_code)]
    #[inline]
    pub fn is_empty(&self) -> bool { self.active == 0 }

    #[inline]
    pub fn contains_key(&self, id: &ElementId) -> bool {
        self.id_to_idx.contains_key(id)
    }

    #[inline]
    pub fn resolve(&self, id: ElementId) -> Option<u32> {
        self.id_to_idx.get(&id).copied()
    }

    #[inline]
    pub fn get(&self, id: &ElementId) -> Option<&ElementNode> {
        let idx = *self.id_to_idx.get(id)?;
        self.nodes.get(idx as usize)?.as_ref()
    }

    #[inline]
    pub fn get_mut(&mut self, id: &ElementId) -> Option<&mut ElementNode> {
        let idx = *self.id_to_idx.get(id)?;
        self.nodes.get_mut(idx as usize)?.as_mut()
    }

    #[inline]
    pub fn get_by_idx(&self, idx: u32) -> Option<&ElementNode> {
        self.nodes.get(idx as usize)?.as_ref()
    }

    #[inline]
    pub fn get_mut_by_idx(&mut self, idx: u32) -> Option<&mut ElementNode> {
        self.nodes.get_mut(idx as usize)?.as_mut()
    }

    pub fn insert(&mut self, id: ElementId, mut node: ElementNode) -> Option<ElementNode> {
        node.id = id;
        if let Some(&idx) = self.id_to_idx.get(&id) {
            let slot = &mut self.nodes[idx as usize];
            return std::mem::replace(slot, Some(node));
        }
        let idx = if let Some(free) = self.free_list.pop() {
            self.nodes[free as usize] = Some(node);
            free
        } else {
            let i = self.nodes.len() as u32;
            self.nodes.push(Some(node));
            i
        };
        self.id_to_idx.insert(id, idx);
        self.active += 1;
        None
    }

    pub fn remove(&mut self, id: &ElementId) -> Option<ElementNode> {
        let idx = self.id_to_idx.remove(id)?;
        let taken = self.nodes.get_mut(idx as usize)?.take();
        if taken.is_some() {
            self.free_list.push(idx);
            self.active -= 1;
        }
        taken
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ElementId, &ElementNode)> {
        self.nodes.iter().filter_map(|slot| {
            slot.as_ref().map(|n| (&n.id, n))
        })
    }

    pub fn iter_idx(&self) -> impl Iterator<Item = (u32, &ElementNode)> {
        self.nodes.iter().enumerate().filter_map(|(i, slot)| {
            slot.as_ref().map(|n| (i as u32, n))
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = &ElementId> {
        self.nodes.iter().filter_map(|slot| slot.as_ref().map(|n| &n.id))
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut ElementNode> {
        self.nodes.iter_mut().filter_map(|slot| slot.as_mut())
    }

    #[allow(dead_code)]
    pub fn par_values_mut(&mut self) -> impl rayon::iter::ParallelIterator<Item = &mut ElementNode> {
        use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
        self.nodes.par_iter_mut().filter_map(|slot| slot.as_mut())
    }

    #[allow(dead_code)]
    #[inline]
    pub fn slot_count(&self) -> usize { self.nodes.len() }
}

impl Default for ElementStorage {
    fn default() -> Self { Self::new() }
}

impl<'a> IntoIterator for &'a ElementStorage {
    type Item = (&'a ElementId, &'a ElementNode);
    type IntoIter = std::iter::FilterMap<
        std::slice::Iter<'a, Option<ElementNode>>,
        fn(&'a Option<ElementNode>) -> Option<(&'a ElementId, &'a ElementNode)>,
    >;
    fn into_iter(self) -> Self::IntoIter {
        fn project<'a>(slot: &'a Option<ElementNode>) -> Option<(&'a ElementId, &'a ElementNode)> {
            slot.as_ref().map(|n| (&n.id, n))
        }
        self.nodes.iter().filter_map(project)
    }
}
