use core::{arch::global_asm, num::NonZeroU32};

use log::warn;

use crate::{device_tree, memory::PhysicalAddress};

struct PsciInfos {
    cpu_on: Option<NonZeroU32>,
}

static mut INFOS: Option<PsciInfos> = None;

pub fn init() {
    let dt_node = match device_tree::get_node("/psci") {
        Some(node) => node,
        None => {
            warn!(target: "psci", "Init failed: no psci node in the device tree");
            return;
        }
    };

    let method = match dt_node.get_property("method") {
        Some(method) => method.buff().consume_str().unwrap(),
        None => {
            warn!(target: "psci", "Init failed: no method property in device tree");
            return;
        }
    };
    if method != "hvc" {
        warn!(target: "psci", "Init failed: unknown method");
        return;
    }

    let cpu_on = dt_node.get_property("cpu_on").map_or_else(
        || {
            warn!(target: "psci", "No cpu_on function");
            None
        },
        |p| {
            let val = p.buff().consume_be_u32().unwrap();
            Some(NonZeroU32::new(val).unwrap())
        },
    );

    let infos = PsciInfos { cpu_on };
    unsafe { INFOS = Some(infos) };
}

global_asm!(
    ".global hvc_call
     hvc_call:
     hvc #0
     ret"
);

unsafe extern "C" {
    unsafe fn hvc_call(func: u32, a: u64, b: u64, c: u64, d: u64) -> u64;
}

#[inline]
pub unsafe fn cpu_on(cpu_id: u32, entry: PhysicalAddress, context: u64) {
    let func = unsafe { &*&raw const INFOS }
        .as_ref()
        .expect("Psci not init")
        .cpu_on
        .expect("No cpu_on func");
    unsafe { hvc_call(func.get(), cpu_id as u64, entry.addr() as u64, context, 0) };
}
