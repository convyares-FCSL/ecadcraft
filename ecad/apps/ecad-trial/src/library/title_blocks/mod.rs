//! Library ▸ Title blocks (brief 055, design 055 §4, §5): the title block and frame editor.

use ecad_model::PageId;

use crate::Trial;

impl Trial {
    /// Open Library ▸ Title blocks; `at`: a Checks row's page and item, selected in its frame.
    pub fn open_title_blocks(&mut self, at: Option<(PageId, u16)>) {
        let _ = at;
        self.library.browser.tab = super::browser::LibraryTab::TitleBlocks;
        self.set_workspace(crate::shell::Workspace::Library);
    }
}
