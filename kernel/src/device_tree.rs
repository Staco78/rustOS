use core::{
    ffi::CStr,
    mem::{size_of, MaybeUninit},
    slice, str,
};

use crate::memory::{vmm::phys_to_virt, PhysicalAddress};
use alloc::{string::String, vec::Vec};

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
        let (a, buff, _): (_, &[u32], _) = unsafe { buff.align_to() };
        assert!(a.is_empty());
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
        assert!(s.magic == MAGIC, "Invalid magic field in dtb");
        assert!(s.version == 17, "Unsupported dtb version");
        s
    }
}

struct Scanner<'a> {
    buff: &'a [u8],
    index: usize,
}

impl<'a> Scanner<'a> {
    fn new(buff: &'a [u8]) -> Self {
        Self { buff, index: 0 }
    }

    fn consume_u32(&mut self) -> u32 {
        assert!(self.index + 4 <= self.buff.len());
        let val = &self.buff[self.index..self.index + 4];
        let val = u32::from_be_bytes(<[u8; 4]>::try_from(val).unwrap());
        self.index += 4;
        val
    }

    #[inline]
    fn consume_str(&mut self) -> &'a str {
        let str = CStr::from_bytes_until_nul(&self.buff[self.index..])
            .unwrap()
            .to_str()
            .unwrap();
        self.index += str.len() + 1;
        str
    }

    // align to 4 bytes
    fn align(&mut self) {
        let modulo = self.index % 4;
        if modulo != 0 {
            self.index += 4 - modulo;
        }
    }
}

struct Parser<'a> {
    buff: &'a [u8],
    header: Header,
}

impl<'a> Parser<'a> {
    fn new(buff: &'a [u8]) -> Self {
        let header = Header::from_buff(buff);
        Self { buff, header }
    }

    fn get_str(&self, index: u32) -> &'a str {
        let start = self.header.off_dt_strings as usize;
        let end = start + self.header.size_dt_strings as usize;
        let buff = &self.buff[start..end];
        let cstr = CStr::from_bytes_until_nul(&buff[index as usize..]).unwrap();
        cstr.to_str().unwrap()
    }

    fn parse_node(&self, scanner: &mut Scanner) -> Node {
        let mut children = Vec::new();
        let mut properties = Vec::new();

        loop {
            match scanner.consume_u32() {
                FDT_BEGIN_NODE => {
                    let name = scanner.consume_str();
                    scanner.align();
                    let node = self.parse_node(scanner);
                    children.push((String::from(name), node));
                    scanner.align();
                }
                FDT_END_NODE => {
                    children.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                    properties.sort_unstable_by(|a: &(String, Vec<u8>), b| a.0.cmp(&b.0));
                    return Node {
                        children,
                        properties,
                    };
                }
                FDT_PROP => {
                    let len = scanner.consume_u32();
                    let nameoff = scanner.consume_u32();
                    let name = self.get_str(nameoff);
                    let mut data = alloc::vec![0; len as usize];
                    data.copy_from_slice(
                        &scanner.buff[scanner.index..scanner.index + len as usize],
                    );
                    properties.push((String::from(name), data));
                    scanner.index += len as usize;
                    scanner.align();
                }
                FDT_NOP => (),
                FDT_END => unimplemented!(),
                _ => unimplemented!(),
            }
        }
    }

    fn parse(&self) -> Node {
        let struct_start = self.header.off_dt_struct as usize;
        let struct_end = struct_start + self.header.size_dt_struct as usize;
        let mut scanner = Scanner::new(&self.buff[struct_start..struct_end]);
        assert!(scanner.consume_u32() == FDT_BEGIN_NODE);
        scanner.consume_str();
        scanner.align();
        let root = self.parse_node(&mut scanner);
        scanner.align();
        assert!(scanner.consume_u32() == FDT_END);
        root
    }
}

// TODO: use custom allocator bc this will never be deallocated
#[derive(Debug)]
pub struct Node {
    children: Vec<(String, Node)>,
    properties: Vec<(String, Vec<u8>)>,
}

impl Node {
    pub fn get_child(&self, name: &str) -> Option<&Node> {
        let r = self.children.binary_search_by(|n| n.0.as_str().cmp(name));
        match r {
            Ok(i) => Some(&self.children[i].1),
            Err(_) => None,
        }
    }

    // find all nodes which start with prefix
    pub fn get_children_by_prefix(&self, prefix: &str) -> Option<&[(String, Node)]> {
        let start_with = |e: &(String, Node)| -> bool { e.0.starts_with(prefix) };

        let mut iter = self.children.iter().enumerate();
        let first = iter.find(|(_, e)| start_with(e)).map(|(i, _)| i)?;

        let mut valid_count = 0;
        while let Some(e) = iter.next() && start_with(e.1) {
            valid_count += 1;
        };

        let last = first + valid_count;
        Some(&self.children[first..=last])
    }

    #[inline]
    pub fn get_property(&self, name: &str) -> Option<&[u8]> {
        let r = self.properties.binary_search_by(|n| n.0.as_str().cmp(name));
        match r {
            Ok(i) => Some(&self.properties[i].1),
            Err(_) => None,
        }
    }

    #[inline]
    pub fn address_cells(&self) -> u32 {
        let prop = self.get_property("#address-cells");
        let prop = match prop {
            Some(prop) => prop,
            None => return 2, // default value
        };
        assert!(prop.len() == size_of::<u32>());
        u32::from_be_bytes(prop.try_into().unwrap())
    }

    #[inline]
    pub fn size_cells(&self) -> u32 {
        let prop = self.get_property("#size-cells");
        let prop = match prop {
            Some(prop) => prop,
            None => return 1, // default value
        };
        assert!(prop.len() == size_of::<u32>());
        u32::from_be_bytes(prop.try_into().unwrap())
    }
}

static mut ROOT_NODE: MaybeUninit<Node> = MaybeUninit::uninit();
static mut BOOT_CPU: MaybeUninit<u32> = MaybeUninit::uninit();

pub fn load(ptr: PhysicalAddress, len: u32) {
    let buff = unsafe { slice::from_raw_parts(phys_to_virt(ptr) as *const u8, len as usize) };
    let parser = Parser::new(buff);
    let root_node = parser.parse();
    unsafe {
        ROOT_NODE.write(root_node);
        BOOT_CPU.write(parser.header.boot_cpuid_phys);
    };
}

pub fn get_root() -> &'static Node {
    unsafe { ROOT_NODE.assume_init_ref() }
}

pub fn get_node(path: &str) -> Option<&'static Node> {
    let mut current_node = get_root();
    let mut iter = path.split('/');
    while let Some(name) = iter.next() {
        current_node = current_node.get_child(name)?;
    }
    Some(current_node)
}

#[inline]
pub fn get_boot_cpu_id() -> u32 {
    unsafe { BOOT_CPU.assume_init_read() }
}
