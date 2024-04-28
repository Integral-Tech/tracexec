// Copyright (c) 2023 Ratatui Developers
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
// associated documentation files (the "Software"), to deal in the Software without restriction,
// including without limitation the rights to use, copy, modify, merge, publish, distribute,
// sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all copies or substantial
// portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
// NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
// OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::borrow::Cow;

use arboard::Clipboard;
use clap::ValueEnum;
use crossterm::event::KeyCode;
use itertools::chain;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  buffer::Buffer,
  layout::{Constraint, Layout, Rect},
  style::{Color, Style, Styled, Stylize},
  text::{Line, Span},
  widgets::{Block, Paragraph, StatefulWidget, Widget, Wrap},
};
use strum::Display;
use tokio::sync::mpsc;

use crate::{
  action::{Action, CopyTarget, Shell},
  cli::{
    args::{ModifierArgs, TracingArgs},
    options::ActivePane,
  },
  event::{Event, TracerEvent},
  printer::PrinterArgs,
  proc::BaselineInfo,
  pty::{PtySize, UnixMasterPty},
};

use super::{event_list::EventList, pseudo_term::PseudoTerminalPane, ui::render_title, Tui};

#[derive(Debug, Clone, PartialEq, Default, ValueEnum, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum AppLayout {
  #[default]
  Horizontal,
  Vertical,
}

pub struct App {
  pub event_list: EventList,
  pub printer_args: PrinterArgs,
  pub term: Option<PseudoTerminalPane>,
  pub root_pid: Option<Pid>,
  pub active_pane: ActivePane,
  pub clipboard: Clipboard,
  pub split_percentage: u16,
  pub layout: AppLayout,
  pub should_handle_internal_resize: bool,
}

impl App {
  pub fn new(
    tracing_args: &TracingArgs,
    modifier_args: &ModifierArgs,
    baseline: BaselineInfo,
    pty_master: Option<UnixMasterPty>,
    active_pane: ActivePane,
    layout: AppLayout,
  ) -> color_eyre::Result<Self> {
    let active_pane = if pty_master.is_some() {
      active_pane
    } else {
      ActivePane::Events
    };
    Ok(Self {
      event_list: EventList::new(baseline),
      printer_args: PrinterArgs::from_cli(tracing_args, modifier_args),
      split_percentage: if pty_master.is_some() { 50 } else { 100 },
      term: if let Some(pty_master) = pty_master {
        Some(PseudoTerminalPane::new(
          PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
          },
          pty_master,
        )?)
      } else {
        None
      },
      root_pid: None,
      active_pane,
      clipboard: Clipboard::new()?,
      layout,
      should_handle_internal_resize: true,
    })
  }

  pub fn shrink_pane(&mut self) {
    if self.term.is_some() {
      self.split_percentage = self.split_percentage.saturating_sub(1).max(10);
    }
  }

  pub fn grow_pane(&mut self) {
    if self.term.is_some() {
      self.split_percentage = self.split_percentage.saturating_add(1).min(90);
    }
  }

  pub async fn run(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    loop {
      // Handle events
      if let Some(e) = tui.next().await {
        if e != Event::Render {
          log::trace!("Received event {e:?}");
        }
        match e {
          Event::ShouldQuit => {
            action_tx.send(Action::Quit)?;
          }
          Event::Key(ke) => {
            if ke.code == KeyCode::Char('s')
              && ke
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            {
              action_tx.send(Action::SwitchActivePane)?;
              // action_tx.send(Action::Render)?;
            } else {
              log::trace!("TUI: Active pane: {}", self.active_pane);
              if self.active_pane == ActivePane::Events {
                match ke.code {
                  KeyCode::Char('q') => {
                    action_tx.send(Action::Quit)?;
                  }
                  KeyCode::Down | KeyCode::Char('j') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::CONTROL {
                      action_tx.send(Action::PageDown)?;
                    } else if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::NextItem)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Up | KeyCode::Char('k') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::CONTROL {
                      action_tx.send(Action::PageUp)?;
                    } else if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::PrevItem)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Left | KeyCode::Char('h') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::CONTROL {
                      action_tx.send(Action::PageLeft)?;
                    } else if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::ScrollLeft)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Right | KeyCode::Char('l')
                    if ke.modifiers != crossterm::event::KeyModifiers::ALT =>
                  {
                    if ke.modifiers == crossterm::event::KeyModifiers::CONTROL {
                      action_tx.send(Action::PageRight)?;
                    } else if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::ScrollRight)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::PageDown => {
                    action_tx.send(Action::PageDown)?;
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::PageUp => {
                    action_tx.send(Action::PageUp)?;
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Char('g') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::GrowPane)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Char('s') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::ShrinkPane)?;
                    }
                    // action_tx.send(Action::Render)?;
                  }
                  KeyCode::Char('c') => {
                    if ke.modifiers == crossterm::event::KeyModifiers::NONE {
                      action_tx.send(Action::CopyToClipboard(CopyTarget::Commandline(
                        Shell::Bash,
                      )))?;
                    }
                  }
                  KeyCode::Char('l') if ke.modifiers == crossterm::event::KeyModifiers::ALT => {
                    action_tx.send(Action::SwitchLayout)?;
                  }
                  _ => {}
                }
              } else {
                action_tx.send(Action::HandleTerminalKeyPress(ke))?;
                // action_tx.send(Action::Render)?;
              }
            }
          }
          Event::Tracer(te) => match te {
            TracerEvent::RootChildSpawn(pid) => {
              self.root_pid = Some(pid);
            }
            te => {
              self.event_list.items.push(te);
              // action_tx.send(Action::Render)?;
            }
          },
          Event::Render => {
            action_tx.send(Action::Render)?;
          }
          Event::Resize(size) => {
            action_tx.send(Action::Resize(size))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Init => {
            // Fix the size of the terminal
            action_tx.send(Action::Resize(tui.size()?.into()))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Error => {}
        }
      }

      // Handle actions
      while let Ok(action) = action_rx.try_recv() {
        if action != Action::Render {
          log::debug!("action: {action:?}");
        }
        match action {
          Action::Quit => {
            return Ok(());
          }
          Action::Render => {
            tui.draw(|f| self.render(f.size(), f.buffer_mut()))?;
          }
          Action::NextItem => {
            self.event_list.next();
          }
          Action::PrevItem => {
            self.event_list.previous();
          }
          Action::PageDown => {
            self.event_list.page_down();
          }
          Action::PageUp => {
            self.event_list.page_up();
          }
          Action::PageLeft => {
            self.event_list.page_left();
          }
          Action::PageRight => {
            self.event_list.page_right();
          }
          Action::HandleTerminalKeyPress(ke) => {
            if let Some(term) = self.term.as_mut() {
              term.handle_key_event(&ke).await;
            }
          }
          Action::Resize(_size) => {
            self.should_handle_internal_resize = true;
          }
          Action::ScrollLeft => {
            self.event_list.scroll_left();
          }
          Action::ScrollRight => {
            self.event_list.scroll_right();
          }
          Action::ShrinkPane => {
            self.shrink_pane();
            self.should_handle_internal_resize = true;
          }
          Action::GrowPane => {
            self.grow_pane();
            self.should_handle_internal_resize = true;
          }
          Action::SwitchLayout => {
            self.layout = match self.layout {
              AppLayout::Horizontal => AppLayout::Vertical,
              AppLayout::Vertical => AppLayout::Horizontal,
            };
            self.should_handle_internal_resize = true;
          }
          Action::SwitchActivePane => {
            self.active_pane = match self.active_pane {
              ActivePane::Events => ActivePane::Terminal,
              ActivePane::Terminal => ActivePane::Events,
            }
          }
          Action::CopyToClipboard(_target) => {
            if let Some(_selected) = self.event_list.state.selected() {
              self.clipboard.set_text("🥰")?;
            }
          }
        }
      }
    }
  }

  pub fn exit(&self, terminate_on_exit: bool, kill_on_exit: bool) -> color_eyre::Result<()> {
    // Close pty master
    self.term.as_ref().inspect(|t| t.exit());
    // Terminate root process
    if terminate_on_exit {
      self.signal_root_process(Signal::SIGTERM)?;
    } else if kill_on_exit {
      self.signal_root_process(Signal::SIGKILL)?;
    }
    Ok(())
  }

  pub fn signal_root_process(&self, sig: Signal) -> color_eyre::Result<()> {
    if let Some(root_pid) = self.root_pid {
      nix::sys::signal::kill(root_pid, sig)?;
    }
    Ok(())
  }
}

impl Widget for &mut App {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // Create a space for header, todo list and the footer.
    let vertical = Layout::vertical([
      Constraint::Length(2),
      Constraint::Min(0),
      Constraint::Length(2),
    ]);
    let [header_area, rest_area, footer_area] = vertical.areas(area);
    let horizontal_constraints = [
      Constraint::Percentage(self.split_percentage),
      Constraint::Percentage(100 - self.split_percentage),
    ];
    let [event_area, term_area] = (if self.layout == AppLayout::Horizontal {
      Layout::horizontal
    } else {
      Layout::vertical
    })(horizontal_constraints)
    .areas(rest_area);
    render_title(header_area, buf, "tracexec event list");

    if event_area.width < 4 || term_area.width < 4 {
      Paragraph::new("Terminal\nor\npane\ntoo\nsmall").render(rest_area, buf);
      return;
    }

    if event_area.height < 4 || term_area.height < 4 {
      Paragraph::new("Terminal or pane too small").render(rest_area, buf);
      return;
    }

    // resize
    if self.should_handle_internal_resize {
      self.should_handle_internal_resize = false;
      // Set the window size of the event list
      self.event_list.max_window_len = event_area.height as usize - 2;
      self.event_list.window = (
        self.event_list.window.0,
        self.event_list.window.0 + self.event_list.max_window_len,
      );
      log::debug!("TUI: set event list window: {:?}", self.event_list.window);
      if let Some(term) = self.term.as_mut() {
        term
          .resize(PtySize {
            rows: term_area.height as u16 - 2,
            cols: term_area.width as u16 - 2,
            pixel_width: 0,
            pixel_height: 0,
          })
          .unwrap();
      }
    }

    let block = Block::default()
      .title("Events")
      .borders(ratatui::widgets::Borders::ALL)
      .border_style(Style::new().fg(if self.active_pane == ActivePane::Events {
        Color::Cyan
      } else {
        Color::White
      }));
    self.event_list.render(block.inner(event_area), buf);
    block.render(event_area, buf);
    if let Some(term) = self.term.as_mut() {
      let block = Block::default()
        .title("Pseudo Terminal")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(
          Style::default().fg(if self.active_pane == ActivePane::Terminal {
            Color::Cyan
          } else {
            Color::White
          }),
        );
      term.render(block.inner(term_area), buf);
      block.render(term_area, buf);
    }
    self.render_help(footer_area, buf);
  }
}

macro_rules! help_item {
  ($key: literal, $desc: literal) => {{
    let mut key_string = String::from("\u{00a0}");
    key_string.push_str($key);
    key_string.push_str("\u{00a0}");
    let mut desc_string = String::from("\u{00a0}");
    desc_string.push_str($desc);
    desc_string.push_str("\u{00a0}\u{200b}");
    [key(key_string), desc(desc_string)]
  }};
}

impl App {
  fn render_help(&self, area: Rect, buf: &mut Buffer) {
    fn key<'a, T>(k: T) -> Span<'a>
    where
      T: Into<Cow<'a, str>>,
      T: Styled<Item = Span<'a>>,
    {
      k.fg(Color::Black).bg(Color::Cyan).bold()
    }
    fn desc<'a, T>(d: T) -> Span<'a>
    where
      T: Into<Cow<'a, str>>,
      T: Styled<Item = Span<'a>>,
    {
      d.fg(Color::Cyan).bg(Color::DarkGray).italic().bold()
    }

    let iter = help_item!("Ctrl+S", "Switch\u{00a0}Pane");
    let iter: Box<dyn Iterator<Item = _>> = if self.active_pane == ActivePane::Events {
      Box::new(chain!(
        iter,
        help_item!("↑/↓/←/→/Pg{Up,Dn}", "Navigate"),
        help_item!("Ctrl+<-/->", "Scroll<->"),
        help_item!("G/S", "Grow/Shrink\u{00a0}Pane"),
        help_item!("Alt+L", "Layout"),
        help_item!("V", "View"),
        help_item!("C", "Copy"),
        help_item!("Q", "Quit")
      ))
    } else {
      Box::new(chain!(iter, help_item!("Ctrl+Shift+R", "FIXME")))
    };

    let line = Line::from_iter(iter);
    Paragraph::new(line)
      .wrap(Wrap { trim: false })
      .centered()
      .render(area, buf);
  }
}
