//! Input/Output abstraction for memory, ROM and I/O mapped registers

use gpu::Gpu;
use ui::{Controller, ButtonState};
use cartridge::Cartridge;

pub mod ram;
pub mod timer;

/// Interconnect struct used by the CPU and GPU to access the ROM, RAM
/// and registers
pub struct Interconnect<'a> {
    /// Cartridge interface
    cartridge:  Cartridge,
    /// internal RAM
    iram:       ram::Ram,
    /// 0-page RAM
    zpage:      ram::Ram,
    /// Timer instance
    timer:      timer::Timer,
    /// GPU instance
    gpu:        Gpu<'a>,
    /// Used to store the value of IO Port when not properly
    /// implemented.
    io:         [u8, ..0x4c],
    /// Enabled interrupts
    it_enabled: Interrupts,
    /// Current DMA source address
    dma_src:    u16,
    /// Current DMA index in OAM
    dma_idx:    u16,
    /// Controller interface
    controller: &'a mut (Controller + 'a),
}

impl<'a> Interconnect<'a> {
    /// Create a new Interconnect
    pub fn new<'n>(cartridge:  Cartridge,
                   gpu:        Gpu<'n>,
                   controller: &'n mut (Controller + 'n)) -> Interconnect<'n> {

        let iram = ram::Ram::new(0x2000);
        let zpage = ram::Ram::new(0x7f);
        let io = [0, ..0x4c];

        let timer = timer::Timer::new();

        let it_enabled = Interrupts::from_register(0);

        Interconnect { cartridge:  cartridge,
                       iram:       iram,
                       zpage:      zpage,
                       timer:      timer,
                       gpu:        gpu,
                       io:         io,
                       it_enabled: it_enabled,
                       dma_src:    0,
                       dma_idx:    map::range_size(map::OAM),
                       controller: controller,
        }
    }

    pub fn reset(&mut self) {
        self.cartridge.reset();
        self.iram.reset();
        self.gpu.reset();
        self.zpage.reset();

        self.timer.reset();

        self.it_enabled = Interrupts::from_register(0);

        self.dma_src = 0;
        self.dma_idx = map::range_size(map::OAM);

        for b in self.io.iter_mut() {
            *b = 0;
        }
    }

    pub fn step(&mut self) {
        self.gpu.step();
        self.dma_step();
        self.timer.step();
    }

    pub fn dma_step(&mut self) {
        let end = map::range_size(map::OAM);

        if self.dma_idx >= end {
            // No dma transfer in progress
            return;
        }

        let b = self.fetch_byte(self.dma_src);
        self.gpu.set_oam(self.dma_idx, b);

        self.dma_src += 1;
        self.dma_idx += 1;
    }

    /// Get byte from peripheral mapped at `addr`
    pub fn fetch_byte(&self, addr: u16) -> u8 {

        if let Some(off) = map::in_range(addr, map::ROM) {
            return self.cartridge.rom_byte(off);
        }

        if let Some(off) = map::in_range(addr, map::VRAM) {
            return self.gpu.vram(off);
        }

        if let Some(off) = map::in_range(addr, map::RAM_BANK) {
            return self.cartridge.ram_byte(off);
        }

        if let Some(off) = map::in_range(addr, map::IRAM) {
            return self.iram.byte(off);
        }

        if let Some(off) = map::in_range(addr, map::IRAM_ECHO) {
            return self.iram.byte(off);
        }

        if let Some(off) = map::in_range(addr, map::OAM) {
            return self.gpu.oam(off);
        }

        if let Some(off) = map::in_range(addr, map::IO) {
            return self.io(off);
        }

        if let Some(off) = map::in_range(addr, map::ZERO_PAGE) {
            return self.zpage.byte(off);
        }

        if addr == map::IEN {
            return self.it_enabled.as_register();
        }

        debug!("Read from unmapped memory {:04x}", addr);
        0
    }

    /// Store `val` into peripheral mapped at `addr`
    pub fn store_byte(&mut self, addr: u16, val: u8) {
        if let Some(off) = map::in_range(addr, map::ROM) {
            return self.cartridge.set_rom_byte(off, val);
        }

        if let Some(off) = map::in_range(addr, map::VRAM) {
            return self.gpu.set_vram(off, val);
        }

        if let Some(off) = map::in_range(addr, map::RAM_BANK) {
            return self.cartridge.set_ram_byte(off, val);
        }

        if let Some(off) = map::in_range(addr, map::IRAM) {
            return self.iram.set_byte(off, val);
        }

        if let Some(off) = map::in_range(addr, map::IRAM_ECHO) {
            return self.iram.set_byte(off, val);
        }

        if let Some(off) = map::in_range(addr, map::OAM) {
            return self.gpu.set_oam(off, val);
        }

        if let Some(off) = map::in_range(addr, map::IO) {
            return self.set_io(off, val);
        }

        if let Some(off) = map::in_range(addr, map::ZERO_PAGE) {
            return self.zpage.set_byte(off, val);
        }

        if addr == map::IEN {
            return self.it_enabled = Interrupts::from_register(val);
        }

        debug!("Write to unmapped memory {:04x}: {:02x}", addr, val);
    }

    /// Return the highest priority active interrupt after
    /// acknowledging it. If no interrupt is pending return `None`.
    pub fn next_interrupt_ack(&mut self) -> Option<Interrupt> {
        if self.it_enabled.vblank && self.gpu.it_vblank() {
            self.gpu.ack_it_vblank();
            Some(Interrupt::VBlank)
        } else if self.it_enabled.lcdc && self.gpu.it_lcd() {
            self.gpu.ack_it_lcd();
            Some(Interrupt::Lcdc)
        } else if self.it_enabled.timer && self.timer.interrupt() {
            self.timer.ack_interrupt();
            Some(Interrupt::Timer)
        } else {
            None
        }
    }

    /// Return the highest priority active Interrupt without
    /// acknowledging it. If no interrupt is pending return `None`.
    pub fn next_interrupt(&mut self) -> Option<Interrupt> {
        if self.it_enabled.vblank && self.gpu.it_vblank() {
            Some(Interrupt::VBlank)
        } else if self.it_enabled.lcdc && self.gpu.it_lcd() {
            Some(Interrupt::Lcdc)
        } else if self.it_enabled.timer && self.timer.interrupt() {
            Some(Interrupt::Timer)
        } else {
            None
        }
    }

    /// Retrieve value from IO port
    fn io(&self, addr: u16) -> u8 {
        match addr {
            io_map::INPUT => {
                let v = self.io[0];

                let buttons = self.controller.state();

                let mut r = 0;

                if v & 0x10 == 0 {
                    r |= match buttons.right {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 0;

                    r |= match buttons.left {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 1;

                    r |= match buttons.up {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 2;


                    r |= match buttons.down {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 3;
                }

                if v & 0x20 == 0 {
                    r |= match buttons.a {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 0;

                    r |= match buttons.b {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 1;

                    r |= match buttons.select {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 2;


                    r |= match buttons.start {
                        ButtonState::Up => 1,
                        _               => 0,
                    } << 3;
                }

                return r;
            }
            io_map::DIV => {
                return self.timer.div();
            }
            io_map::TIMA => {
                return self.timer.counter();
            }
            io_map::TMA => {
                return self.timer.modulo();
            }
            io_map::TAC => {
                return self.timer.config();
            }
            io_map::DMA => {
                return self.dma_addr();
            }
            io_map::IF => {
                return Interrupts {
                    vblank: self.gpu.it_vblank(),
                    lcdc:   self.gpu.it_lcd(),
                    timer:  self.timer.interrupt(),
                    serial: false,
                    button: false,
                }.as_register();
            }
            io_map::LCD_STAT => {
                return self.gpu.stat()
            }
            io_map::LCD_SCY => {
                return self.gpu.scy()
            }
            io_map::LCD_SCX => {
                return self.gpu.scx()
            }
            io_map::LCDC => {
                return self.gpu.lcdc()
            }
            io_map::LCD_LY => {
                return self.gpu.line()
            }
            io_map::LCD_LYC => {
                return self.gpu.lyc()
            }
            io_map::LCD_BGP => {
                return self.gpu.bgp()
            }
            io_map::LCD_OBP0 => {
                return self.gpu.obp0()
            }
            io_map::LCD_OBP1 => {
                return self.gpu.obp1()
            }
            io_map::LCD_WY => {
                return self.gpu.wy()
            }
            io_map::LCD_WX => {
                return self.gpu.wx()
            }
            _ => {
                debug!("Unhandled IO read from 0x{:04x}", 0xff00 | addr);
            }
        }

        self.io[(addr & 0xff) as uint]
    }

    /// Set value of IO port
    fn set_io(&mut self, addr: u16, val: u8) {
        self.io[(addr & 0xff) as uint] = val;

        match addr {
            io_map::INPUT => {
                self.controller.update();
            }
            io_map::DIV => {
                return self.timer.reset_div();
            }
            io_map::TIMA => {
                return self.timer.set_counter(val);
            }
            io_map::TMA => {
                return self.timer.set_modulo(val);
            }
            io_map::TAC => {
                return self.timer.set_config(val);
            }
            io_map::DMA => {
                return self.start_dma(val);
            }
            io_map::IF => {
                let f = Interrupts::from_register(val);

                // Explicit writes to the Interrupt Flag register
                // force the interrupt status
                self.gpu.force_it_vblank(f.vblank);
                self.gpu.force_it_lcd(f.lcdc);
                self.timer.force_interrupt(f.timer);
            }
            io_map::LCD_STAT => {
                return self.gpu.set_stat(val);
            }
            io_map::LCD_SCY => {
                return self.gpu.set_scy(val);
            }
            io_map::LCD_SCX => {
                return self.gpu.set_scx(val);
            }
            io_map::LCDC => {
                return self.gpu.set_lcdc(val);
            },
            io_map::LCD_LY => {
                // Read Only
            },
            io_map::LCD_LYC => {
                return self.gpu.set_lyc(val);
            }
            io_map::LCD_BGP => {
                return self.gpu.set_bgp(val);
            }
            io_map::LCD_OBP0 => {
                return self.gpu.set_obp0(val);
            }
            io_map::LCD_OBP1 => {
                return self.gpu.set_obp1(val);
            }
            io_map::LCD_WY => {
                return self.gpu.set_wy(val);
            }
            io_map::LCD_WX => {
                return self.gpu.set_wx(val);
            }
            _ => {
                debug!("Unhandled IO write to 0x{:04x}: 0x{:02x}",
                       0xff00 | addr, val);
            }
        }
    }

    /// Return the base of the last DMA transfer (only the high byte,
    /// the low byte is always 0)
    fn dma_addr(&self) -> u8 {
        (self.dma_src >> 8) as u8
    }

    /// Start a new transfer from (`src` << 8) into OAM
    fn start_dma(&mut self, src: u8) {
        self.dma_idx = 0;
        self.dma_src = (src as u16) << 8;

        self.dma_step();
    }
}

/// The various sources of interrupt, from highest to lowest priority
pub enum Interrupt {
    /// GPU entered vertical blanking
    VBlank,
    /// Configurable LCD Controller interrupt
    Lcdc,
    /// Timer overflow
    Timer,
    // TODO: implement other interrupts
}

/// GB Interrupts, from highest to lowest priority
struct Interrupts {
    /// GPU entered vertical blanking
    vblank: bool,
    /// Configurable LCDC interrupt
    lcdc:   bool,
    /// Timer overflow interrupt
    timer:  bool,
    /// Serial I/O done
    serial: bool,
    /// P10-13 transited from high to low (user pressed button)
    button: bool,
}

impl Interrupts {
    /// Convert IE/IF register to Interrupt struct
    fn from_register(reg: u8) -> Interrupts {
        Interrupts {
            vblank: reg & 0x01 != 0,
            lcdc:   reg & 0x02 != 0,
            timer:  reg & 0x04 != 0,
            serial: reg & 0x08 != 0,
            button: reg & 0x10 != 0,
        }
    }

    /// Convert Interrupts into IE/IF register
    fn as_register(&self) -> u8 {
        let mut r = 0;

        r |= (self.vblank as u8) << 0;
        r |= (self.lcdc   as u8) << 1;
        r |= (self.timer  as u8) << 2;
        r |= (self.serial as u8) << 3;
        r |= (self.button as u8) << 4;

        r
    }
}

mod map {
    //! Game Boy memory map. Memory ranges are inclusive.

    /// ROM
    pub const ROM:       (u16, u16) = (0x0000, 0x7fff);
    /// Video RAM
    pub const VRAM:      (u16, u16) = (0x8000, 0x9fff);
    /// RAM Bank N
    pub const RAM_BANK:  (u16, u16) = (0xa000, 0xbfff);
    /// Internal RAM
    pub const IRAM:      (u16, u16) = (0xc000, 0xdfff);
    /// Internal RAM echo
    pub const IRAM_ECHO: (u16, u16) = (0xe000, 0xfdff);
    /// Object Attribute Memory
    pub const OAM:       (u16, u16) = (0xfe00, 0xfe9f);
    /// IO ports
    pub const IO:        (u16, u16) = (0xff00, 0xff4b);
    /// Zero page memory
    pub const ZERO_PAGE: (u16, u16) = (0xff80, 0xfffe);
    /// Interrupt Enable register
    pub const IEN:       u16        = 0xffff;

    /// Return `Some(offset)` if the given address is in the inclusive
    /// range `range`, Where `offset` is an u16 equal to the offset of
    /// `addr` within the `range`.
    pub fn in_range(addr: u16, range: (u16, u16)) -> Option<u16> {
        let (first, last) = range;

        if addr >= first && addr <= last {
            Some(addr - first)
        } else {
            None
        }
    }

    /// Return the size of `range` in bytes
    pub fn range_size(range: (u16, u16)) -> u16 {
        let (first, last) = range;

        return last - first + 1;
    }
}

mod io_map {
    //! IO Address Map (offset from 0xff00)

    /// Input button matrix control
    pub const INPUT:    u16 = 0x00;
    /// 16.384kHz free-running counter. Writing to it resets it to 0.
    pub const DIV:      u16 = 0x04;
    /// Configurable timer counter
    pub const TIMA:     u16 = 0x05;
    /// Configurable timer modulo (value reloaded in the counter after
    /// oveflow)
    pub const TMA:      u16 = 0x06;
    /// Timer control register
    pub const TAC:      u16 = 0x07;
    /// Interrupt Flag register
    pub const IF:       u16 = 0x0f;
    /// LCD Control
    pub const LCDC:     u16 = 0x40;
    /// LCDC Status + IT selection
    pub const LCD_STAT: u16 = 0x41;
    /// LCDC Background Y position
    pub const LCD_SCY:  u16 = 0x42;
    /// LCDC Background X position
    pub const LCD_SCX:  u16 = 0x43;
    /// Currently displayed line
    pub const LCD_LY:   u16 = 0x44;
    /// Currently line compare
    pub const LCD_LYC:  u16 = 0x45;
    /// DMA transfer from ROM/RAM to OAM
    pub const DMA:      u16 = 0x46;
    /// Background palette
    pub const LCD_BGP:  u16 = 0x47;
    /// Sprite palette 0
    pub const LCD_OBP0: u16 = 0x48;
    /// Sprite palette 1
    pub const LCD_OBP1: u16 = 0x49;
    /// Window Y position
    pub const LCD_WY:   u16 = 0x4a;
    /// Window X position + 7
    pub const LCD_WX:   u16 = 0x4b;

}
