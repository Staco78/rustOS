use core::{
    arch::global_asm,
    ffi::CStr,
    sync::atomic::{AtomicU32, Ordering},
};

use cortex_a::registers::{MAIR_EL1, SCTLR_EL1, TCR_EL1, TTBR1_EL1, VBAR_EL1};
use funcs::current_process;
use log::{info, trace};
use tock_registers::interfaces::Readable;

use crate::{
    device_tree::{self, Node},
    interrupts,
    memory::{
        vmm::{virt_to_phys, vmm, MapOptions, MapSize},
        AddrSpaceSelector, MemoryUsage, PhysicalAddress, VirtualAddress, VirtualAddressSpace,
        PAGE_SIZE,
    },
    psci,
    scheduler::{funcs, thread::Thread, SCHEDULER},
};

global_asm!(include_str!("ap_start.S"));

fn dt_iter_cpus<CB>(mut cb: CB)
where
    CB: FnMut(u32, &Node),
{
    let cpus = device_tree::get_node("cpus").expect("No cpus node in device tree");
    let (address_cells, size_cells) = (cpus.address_cells(), cpus.size_cells());
    assert!(address_cells == 1 && size_cells == 0); // we don't support others cells size
    let cpu_nodes = cpus.get_children_by_prefix("cpu@").unwrap();

    for (_, cpu) in cpu_nodes {
        let reg = cpu.get_property("reg").unwrap();
        assert!(reg.len() == 4);
        let id = u32::from_be_bytes(reg.try_into().unwrap());
        cb(id, cpu);
    }
}

// return main core id
pub fn register_cpus() -> u32 {
    dt_iter_cpus(|id, _| {
        let is_main_cpu = id == device_tree::get_boot_cpu_id();
        SCHEDULER.register_cpu(id, is_main_cpu);
    });
    device_tree::get_boot_cpu_id()
}

pub fn start_cpus() {
    let mut low_addr_space = VirtualAddressSpace::create_user().unwrap();
    for i in 0..4 {
        vmm()
            .map_page(
                i * 1024 * 1024 * 1024,
                i * 1024 * 1024 * 1024,
                MapOptions::default_size(MapSize::Size1GB),
                AddrSpaceSelector::Unlocked(&mut low_addr_space),
            )
            .unwrap();
    }

    dt_iter_cpus(|id, cpu| {
        let is_main_cpu = id == device_tree::get_boot_cpu_id();
        if !is_main_cpu {
            start_cpu(id, cpu, &mut low_addr_space);
        }
    })
}

fn start_cpu(id: u32, node: &device_tree::Node, low_addr_space: &mut VirtualAddressSpace) {
    let enable_method = node.get_property("enable-method").unwrap();
    let enable_method = CStr::from_bytes_with_nul(enable_method)
        .unwrap()
        .to_str()
        .unwrap();
    match enable_method {
        "psci" => start_cpu_psci(id, low_addr_space),
        _ => unimplemented!("Unknown enable method"),
    }
}

extern "C" {
    fn ap_start(); // never call that
}

#[repr(C)]
struct StartInfos {
    id: u32,
    has_started: AtomicU32,
    ttbr0: PhysicalAddress,
    ttbr1: PhysicalAddress,
    stack_ptr: VirtualAddress,
    vbar: u64,
    mair: u64,
    tcr: u64,
    sctlr: u64,
}

fn start_cpu_psci(id: u32, low_addr_space: &mut VirtualAddressSpace) {
    trace!(target: "smp", "Starting cpu {id} with psci");
    let entry = ap_start as usize;
    let entry = virt_to_phys(entry).unwrap();

    let start_infos = StartInfos {
        id,
        has_started: AtomicU32::new(0),
        ttbr0: virt_to_phys(low_addr_space.ptr as usize).unwrap(),
        ttbr1: TTBR1_EL1.get_baddr() as usize,
        stack_ptr: alloc_ap_stack(),
        vbar: VBAR_EL1.get(),
        mair: MAIR_EL1.get(),
        tcr: TCR_EL1.get(),
        sctlr: SCTLR_EL1.get(),
    };

    let ptr = (&start_infos as *const StartInfos).addr();
    let ptr = virt_to_phys(ptr).unwrap();

    unsafe { psci::cpu_on(id as u32, entry as u64, ptr as u64) };

    while start_infos.has_started.load(Ordering::Acquire) != 1 {
        core::hint::spin_loop();
    }
}

#[inline]
fn alloc_ap_stack() -> VirtualAddress {
    vmm()
        .alloc_pages(16, MemoryUsage::KernelHeap, AddrSpaceSelector::kernel())
        .unwrap()
        + 16 * PAGE_SIZE
}

#[no_mangle]
extern "C" fn ap_main(id: u32) -> ! {
    info!(target: "smp", "Core {id} online");
    interrupts::chip().init_ap();
    SCHEDULER.start(id, up);
}

fn up() -> ! {
    Thread::new(current_process(), a, false).unwrap().start();
    loop {}
}

fn a() -> ! {
    loop {}
}