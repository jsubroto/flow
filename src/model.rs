pub struct Card {
    pub id: String,
    pub title: String,
    pub description: String,
}

pub struct Column {
    pub id: String,
    pub title: String,
    pub cards: Vec<Card>,
}

pub struct Board {
    pub columns: Vec<Column>,
}
