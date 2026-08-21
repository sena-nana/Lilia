use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditorPosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditorCursor {
    pub position: TextEditorPosition,
    pub selection: Option<TextEditorPosition>,
}

#[derive(Clone, Debug, Default)]
struct Inner {
    text: String,
    cursor: TextEditorCursor,
}

#[derive(Clone, Debug, Default)]
pub struct TextEditorState {
    inner: RefCell<Inner>,
}

impl PartialEq for TextEditorState {
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text()
    }
}

impl TextEditorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            inner: RefCell::new(Inner {
                text: text.into(),
                cursor: TextEditorCursor::default(),
            }),
        }
    }

    pub fn text(&self) -> String {
        self.inner.borrow().text.clone()
    }

    pub fn set_text(&self, value: impl AsRef<str>) {
        let mut inner = self.inner.borrow_mut();
        inner.text = value.as_ref().to_owned();
    }

    pub fn line_count(&self) -> usize {
        self.inner.borrow().text.split('\n').count().max(1)
    }

    pub fn cursor(&self) -> TextEditorCursor {
        self.inner.borrow().cursor
    }

    pub fn move_to(&self, cursor: TextEditorCursor) {
        self.inner.borrow_mut().cursor = cursor;
    }

    pub fn clear(&self) {
        self.set_text("");
    }

    pub fn perform(&self, action: impl Into<String>) {
        self.set_text(action.into());
    }
}
