use crate::widget::{Element, ElementId, ElementTree};

pub trait ElementVisitor {
    fn visit(&mut self, element: &dyn Element);
    fn visit_mut(&mut self, element: &mut dyn Element);
}

pub fn walk_tree<F>(tree: &ElementTree, root_id: ElementId, mut f: F)
where
    F: FnMut(&dyn Element),
{
    fn walk_recursive<F>(tree: &ElementTree, id: ElementId, f: &mut F)
    where
        F: FnMut(&dyn Element),
    {
        if let Some(element) = tree.get(id) {
            f(element);
            let children: Vec<ElementId> = element.children().to_vec();
            for child_id in children {
                walk_recursive(tree, child_id, f);
            }
        }
    }
    walk_recursive(tree, root_id, &mut f);
}
