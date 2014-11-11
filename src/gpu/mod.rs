//! Game Boy GPU emulation

use std::fmt::{Show, Formatter, FormatError};

use ui::Display;

/// GPU state.
pub struct Gpu<'a> {
    /// Current line. [0,143] is active video, [144,153] is blanking.
    line: u8,
    /// Position on the current line.
    col: u16,
    /// Object attritube memory
    oam: [u8, ..0xa0],
    /// Emulator Display
    display: &'a mut Display + 'a,
}

/// Current GPU mode
#[deriving(Show)]
pub enum Mode {
    /// In horizontal blanking
    HBlank = 0,
    /// In vertical blanking
    VBlank = 1,
    /// Accessing sprite memory, Sprite attributes RAM [0xfe00, 0xfe9f]
    /// can't be accessed
    Prelude = 2,
    /// Accessing sprite memory and video memory [0x8000, 0x9fff],
    /// both can't be accessed from CPU
    Active = 3,
}

impl<'a> Gpu<'a> {
    /// Create a new Gpu instance.
    pub fn new<'n>(display: &'n mut Display) -> Gpu<'n> {
        Gpu { line:    0,
              col:     0,
              oam:     [0xca, ..0xa0],
              display: display,
        }
    }

    /// Reset the GPU state to power up values
    pub fn reset(&mut self) {
        self.line = 0;
        self.col  = 0;
        self.oam  = [0xca, ..0xa0];
    }

    /// Called at each tick of the system clock. Move the emulated
    /// state one step forward.
    pub fn step(&mut self) {

        //println!("{}", *self);

        if self.col < 456 {
            self.col += 1;
        } else {
            self.col = 0;

            // Move on to the next line
            if self.line < 154 {
                self.line += 1;

                if self.line == 144 {
                    // We're entering blanking, we're done drawing the
                    // current frame
                    self.end_of_frame()
                }

            } else {
                // New frame
                self.line = 0;
            }
        }
    }

    /// Return current GPU mode
    pub fn get_mode(&self) -> Mode {
        if self.line < 144 {
            match self.col {
                0  ... 79  => Prelude,
                80 ... 172 => Active,
                _          => HBlank,
            }
        } else {
            VBlank
        }
    }

    /// Return number of line currently being drawn
    pub fn get_line(&self) -> u8 {
        self.line
    }

    /// Called when the last line of the active display has been drawn
    fn end_of_frame(&mut self) {
        self.display.flip();
    }

    /// Get byte from OAM
    pub fn get_oam(&self, addr: u8) -> u8 {
        match self.get_mode() {
            Prelude | Active => panic!("OAM access while in use {:02x}", addr),
            _                => self.oam[(addr & 0xff) as uint]
        }
    }

    /// Set byte in OAM
    pub fn set_oam(&mut self, addr: u8, val: u8) {
        match self.get_mode() {
            Prelude | Active => panic!("OAM access while in use {:02x}", addr),
            _                => self.oam[(addr & 0xff) as uint] = val,
        }
    }

}

impl<'a> Show for Gpu<'a> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FormatError> {
        try!(write!(f, "Gpu at ({}, {}) [{}] ", self.col, self.line, self.get_mode()));

        Ok(())
    }
}
