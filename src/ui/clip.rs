#[derive(Default)]
pub struct Clip {
    board: Option<arboard::Clipboard>,
}

impl Clip {
    fn board(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.board.is_none() {
            self.board = arboard::Clipboard::new().ok();
        }
        self.board.as_mut()
    }

    pub fn copy(&mut self, text: &str) -> bool {
        self.board()
            .is_some_and(|board| board.set_text(text.to_owned()).is_ok())
    }
}
