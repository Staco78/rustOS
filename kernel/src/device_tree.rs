use core::{ffi::CStr, str};

use log::warn;

use crate::utils::sync_once_cell::SyncOnceCell;

static DTB: SyncOnceCell<&'static [u8]> = SyncOnceCell::new();
static ROOT_NODE: SyncOnceCell<Node> = SyncOnceCell::new();
static HEADER: SyncOnceCell<Header> = SyncOnceCell::new();

const MAGIC: u32 = 0xd00dfeed;
const FDT_BEGIN_NODE: u32 = 0x01;
const FDT_END_NODE: u32 = 0x02;
const FDT_PROP: u32 = 0x03;
const FDT_NOP: u32 = 0x04;
const FDT_END: u32 = 0x09;

#[repr(C)]
#[derive(Debug)]
struct Header {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

impl Header {
    fn from_buff(buff: &[u8]) -> Self {
        let buff_len = buff.len();
        let (a, buff, _): (_, &[u32], _) = unsafe { buff.align_to() };
        assert!(a.is_empty(), "Misaligned DTB buff");
        let s = Self {
            magic: u32::from_be(buff[0]),
            totalsize: u32::from_be(buff[1]),
            off_dt_struct: u32::from_be(buff[2]),
            off_dt_strings: u32::from_be(buff[3]),
            off_mem_rsvmap: u32::from_be(buff[4]),
            version: u32::from_be(buff[5]),
            last_comp_version: u32::from_be(buff[6]),
            boot_cpuid_phys: u32::from_be(buff[7]),
            size_dt_strings: u32::from_be(buff[8]),
            size_dt_struct: u32::from_be(buff[9]),
        };
        assert_eq!(s.magic, MAGIC, "Invalid magic field in dtb");
        assert_eq!(s.version, 17, "Unsupported dtb version");
        assert_eq!(s.totalsize as usize, buff_len, "Invalid DTB buff len");
        s
    }

    fn get_struct_buff(&self, buff: &'static [u8]) -> Buff {
        let data = &buff[self.off_dt_struct as usize
            ..self.off_dt_struct as usize + self.size_dt_struct as usize];
        Buff::new(data)
    }

    fn get_str_buff(&self, buff: &'static [u8]) -> &'static StrBuff {
        let data = &buff[self.off_dt_strings as usize
            ..self.off_dt_strings as usize + self.size_dt_strings as usize];
        StrBuff::new(data)
    }
}

pub struct Buff {
    data: &'static [u8],
    index: usize,
}

impl Buff {
    fn new(data: &'static [u8]) -> Self {
        Self { data, index: 0 }
    }

    pub fn consume_be_u32(&mut self) -> Option<u32> {
        if self.index + 4 > self.data.len() {
            return None;
        }
        let val = u32::from_be_bytes(self.data[self.index..self.index + 4].try_into().unwrap());
        self.index += 4;
        Some(val)
    }

    pub fn consume_be_u64(&mut self) -> Option<u64> {
        if self.index + 8 > self.data.len() {
            return None;
        }
        let val = u64::from_be_bytes(self.data[self.index..self.index + 8].try_into().unwrap());
        self.index += 8;
        Some(val)
    }

    pub fn consume_str(&mut self) -> Option<&'static str> {
        let cstr = CStr::from_bytes_until_nul(&self.data[self.index..]).ok()?;
        self.index += cstr.to_bytes_with_nul().len();
        cstr.to_str().ok()
    }

    pub fn consume_slice(&mut self, len: usize) -> Option<&'static [u8]> {
        if self.index + len > self.data.len() {
            return None;
        }
        let slice = &self.data[self.index..self.index + len];
        self.index += len;
        Some(slice)
    }

    pub fn advance_by(&mut self, bytes_count: usize) -> Result<(), ()> {
        self.index += bytes_count;
        if self.index >= self.data.len() {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Align the buff to 4 bytes.
    fn align(&mut self) {
        self.index = self.index.next_multiple_of(4);
    }
}

#[derive(Debug)]
#[repr(transparent)]
struct StrBuff {
    buff: [u8],
}

impl StrBuff {
    fn new(buff: &'static [u8]) -> &'static Self {
        unsafe { &*(buff as *const _ as *const Self) }
    }

    fn get_str(&self, index: usize) -> Option<&str> {
        CStr::from_bytes_until_nul(&self.buff[index..])
            .ok()?
            .to_str()
            .ok()
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    name: &'static str,
    data: &'static [u8],
    str_buff: &'static StrBuff,
    address_cells: u32,
    size_cells: u32,
}

impl Node {
    pub fn properties(&self) -> NodePropertiesIterator {
        NodePropertiesIterator {
            buff: Buff::new(self.data),
            str_buff: self.str_buff,
            address_cells: self.address_cells,
            size_cells: self.size_cells,
        }
    }

    pub fn children(&self) -> NodeChildrenIterator {
        NodeChildrenIterator {
            buff: Buff::new(self.data),
            str_buff: self.str_buff,
            address_cells: self.address_cells,
            size_cells: self.size_cells,
        }
    }

    #[inline]
    pub fn get_child(&self, name: &str) -> Option<Node> {
        self.children().find(|n| n.name == name)
    }

    #[inline]
    pub fn get_property(&self, name: &str) -> Option<Property> {
        self.properties().find(|n| n.name == name)
    }

    #[inline(always)]
    pub fn address_cells(&self) -> usize {
        self.address_cells as usize
    }

    #[inline(always)]
    pub fn size_cells(&self) -> usize {
        self.size_cells as usize
    }

    #[inline(always)]
    pub fn name(&self) -> &'static str {
        self.name
    }
}

pub struct NodePropertiesIterator {
    buff: Buff,
    str_buff: &'static StrBuff,
    address_cells: u32,
    size_cells: u32,
}

impl Iterator for NodePropertiesIterator {
    type Item = Property;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.buff.consume_be_u32()?;
        match token {
            FDT_PROP => {
                let len = self.buff.consume_be_u32()?;
                let name_off = self.buff.consume_be_u32()?;
                let name = self.str_buff.get_str(name_off as usize)?;
                let data = self.buff.consume_slice(len as usize)?;
                self.buff.align();
                Some(Property { name, data })
            }
            FDT_BEGIN_NODE => {
                get_node_in_buff(
                    &mut self.buff,
                    self.str_buff,
                    self.address_cells,
                    self.size_cells,
                )
                .expect("Node parsing failed");
                self.next()
            }
            FDT_NOP => self.next(),
            FDT_END_NODE => None,
            FDT_END => {
                panic!("Token `FDT_END` invalid here");
            }
            _ => {
                panic!("Invalid token {}", token);
            }
        }
    }
}

pub struct NodeChildrenIterator {
    buff: Buff,
    str_buff: &'static StrBuff,
    address_cells: u32,
    size_cells: u32,
}

impl Iterator for NodeChildrenIterator {
    type Item = Node;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.buff.consume_be_u32()?;
        match token {
            FDT_PROP => {
                let len = self.buff.consume_be_u32()?;
                self.buff.advance_by(len as usize + 4).ok()?;
                self.buff.align();
                self.next()
            }
            FDT_BEGIN_NODE => {
                let node = get_node_in_buff(
                    &mut self.buff,
                    self.str_buff,
                    self.address_cells,
                    self.size_cells,
                )
                .expect("Node parsing failed");
                Some(node)
            }
            FDT_NOP => self.next(),
            FDT_END_NODE => None,
            FDT_END => {
                panic!("Token `FDT_END` invalid here");
            }
            _ => {
                panic!("Invalid token {}", token);
            }
        }
    }
}

#[derive(Debug)]
pub struct Property {
    name: &'static str,
    data: &'static [u8],
}

impl Property {
    #[inline(always)]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline(always)]
    pub fn buff(&self) -> Buff {
        Buff::new(self.data)
    }
}

/// Try to get a node in `buff`. The `FDT_BEGIN_NODE` should already have been consumed.
fn get_node_in_buff(
    buff: &mut Buff,
    str_buff: &'static StrBuff,
    mut address_cells: u32,
    mut size_cells: u32,
) -> Option<Node> {
    let name = buff.consume_str()?;
    buff.align();
    let start_idx = buff.index;

    loop {
        let token = buff.consume_be_u32()?;
        match token {
            FDT_BEGIN_NODE => {
                let _node = get_node_in_buff(buff, str_buff, address_cells, size_cells)?;
                buff.align();
            }
            FDT_END_NODE => {
                buff.align();
                return Some(Node {
                    name,
                    data: &buff.data[start_idx..buff.index],
                    str_buff,
                    address_cells,
                    size_cells,
                });
            }
            FDT_PROP => {
                let len = buff.consume_be_u32()?;
                let name_off = buff.consume_be_u32()?;
                let data = buff.consume_slice(len as usize)?;
                buff.align();
                let name = str_buff.get_str(name_off as usize)?;
                match name {
                    "#size-cells" | "#address-cells" => {
                        assert!(len == 4);
                        let val = u32::from_be_bytes(data.try_into().unwrap());
                        match name {
                            "#size-cells" => size_cells = val,
                            "#address-cells" => address_cells = val,
                            _ => unreachable!(),
                        }
                    }
                    _ => (),
                }
            }
            FDT_NOP => (),
            FDT_END => {
                warn!(target: "dtb", "Token `FDT_END` invalid here");
                return None;
            }
            _ => {
                warn!(target: "dtb", "Invalid token {}", token);
                return None;
            }
        }
    }
}

pub fn init(dtb: &'static [u8]) -> Result<(), ()> {
    let header = Header::from_buff(dtb);
    let mut struct_buff = header.get_struct_buff(dtb);
    let token = struct_buff.consume_be_u32().ok_or(())?;
    assert_eq!(token, FDT_BEGIN_NODE);
    let str_buff = header.get_str_buff(dtb);

    let root_node = get_node_in_buff(&mut struct_buff, str_buff, 0, 0).ok_or(())?;

    unsafe { DTB.set(dtb).unwrap() };
    unsafe { ROOT_NODE.set(root_node).unwrap() };
    unsafe { HEADER.set(header).unwrap() };

    Ok(())
}

/// Find a node with the path `path`. `path` should start with '/'.
pub fn get_node(path: &str) -> Option<Node> {
    debug_assert_eq!(path.chars().next(), Some('/'));
    let mut current_node = ROOT_NODE
        .get()
        .expect("Device tree not initialized")
        .clone();
    for name in path[1..].split('/') {
        current_node = current_node.get_child(name)?;
    }
    Some(current_node)
}

/// The same as `get_node` except that instead of == comparaison
/// each node's name should start with the path component and an '@'.
pub fn get_node_weak(path: &str) -> Option<Node> {
    debug_assert_eq!(path.chars().next(), Some('/'));
    let mut current_node = ROOT_NODE
        .get()
        .expect("Device tree not initialized")
        .clone();
    for name in path[1..].split('/') {
        current_node = current_node.children().find(|c| {
            c.name().starts_with(name)
                && c.name()
                    .chars()
                    .nth(name.len())
                    .map_or_else(|| true, |c| c == '@')
        })?;
    }
    Some(current_node)
}

pub fn get_boot_cpu_id() -> u32 {
    let header = HEADER.get().expect("Device tree not initialized");
    header.boot_cpuid_phys
}
