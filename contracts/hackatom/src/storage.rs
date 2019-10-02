use std::vec::Vec;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char};

extern "C" {
    fn c_read() -> *mut c_char;
    fn c_write(string: *mut c_char);
}

pub trait Storage {
    fn get_state(&self) -> Option<Vec<u8>>;
    fn set_state(&mut self, state: Vec<u8>);
}

#[cfg(target_arch = "wasm32")]
pub struct ExternalStorage {}

#[cfg(target_arch = "wasm32")]
impl Storage for &mut ExternalStorage {
    fn get_state(&self) -> Option<Vec<u8>> {
        unsafe {
            let ptr = c_read();
            if ptr.is_null() {
                return None;
            }
            let state = CStr::from_ptr(ptr).to_bytes().to_vec();
            return Some(state);
        }
    }

    fn set_state(&mut self, state: Vec<u8>) {
        unsafe {
            c_write(CString::new(state).unwrap().into_raw());
        }
    }
}

#[cfg(test)]
pub struct MockStorage {
    data: Option<Vec<u8>>
}

#[cfg(test)]
impl MockStorage {
    pub fn new() -> MockStorage {
        MockStorage{data: None}
    }
}

#[cfg(test)]
impl Storage for &mut MockStorage {
    fn get_state(&self) -> Option<Vec<u8>> {
        match self.data {
            Some(v) => Some(v.clone()),
            None => None,
        }
    }

    fn set_state(&mut self, state: Vec<u8>) {
        self.data = Some(state);
    }
}