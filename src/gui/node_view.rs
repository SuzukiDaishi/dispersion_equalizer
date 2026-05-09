use crate::model::NodeType;

pub fn node_type_label(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Bell => "Bell",
        NodeType::LowShelf => "Low Shelf",
        NodeType::HighShelf => "High Shelf",
        NodeType::Scale => "Scale",
    }
}
