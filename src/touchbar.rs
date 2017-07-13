extern crate objc;
extern crate objc_foundation;
extern crate objc_id;
extern crate cocoa;

use super::interface::*;

use std::rc::Rc;
use std::cell::Cell;
use std::sync::{Once, ONCE_INIT};
use std::collections::BTreeMap;

use objc::Message;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use self::objc_foundation::{INSObject, NSObject};
use self::cocoa::base::{nil, YES, NO, SEL};
use self::cocoa::foundation::NSString;
use self::cocoa::foundation::{NSRect, NSPoint, NSSize};
use self::cocoa::appkit::{NSApp, NSImage};
use self::objc_id::Id;
use self::objc_id::Shared;

const IDENT_PREFIX: &'static str = "com.trevorbentley.";

#[link(name = "DFRFoundation", kind = "framework")]
extern {
    pub fn DFRSystemModalShowsCloseBoxWhenFrontMost(x: i8);
    pub fn DFRElementSetControlStripPresenceForIdentifier(n: *mut Object, x: i8);
}

#[cfg(feature = "libc")]
pub mod print_nsstring {
    extern crate libc;
    use objc::runtime::Object;
    use std::ffi::CStr;
    #[allow(dead_code)]
    fn print_nsstring(str: *mut Object) {
        unsafe {
            let cstr: *const libc::c_char = msg_send![str, UTF8String];
            let rstr = CStr::from_ptr(cstr).to_string_lossy().into_owned();
            info!("{}", rstr);
        }
    }
}


struct Scrubber {
    data: Rc<TScrubberData>,
    _item: ItemId,
    _scrubber: ItemId,
    _ident: Ident,
}

pub struct RustTouchbarDelegateWrapper {
    objc: Id<ObjcAppDelegate, Shared>,
    next_item_id: Cell<u64>,
    bar_obj_map: BTreeMap<ItemId, Ident>,
    control_obj_map: BTreeMap<ItemId, ItemId>,
    scrubber_obj_map: BTreeMap<ItemId, Scrubber>,
    button_cb_map: BTreeMap<ItemId, ButtonCb>,
    slider_cb_map: BTreeMap<ItemId, SliderCb>,
}
pub type Touchbar = Box<RustTouchbarDelegateWrapper>;

impl RustTouchbarDelegateWrapper {
    fn generate_ident(&mut self) -> u64 {
        unsafe {
            // Create string identifier
            let next_item_id = self.next_item_id.get();
            self.next_item_id.set(next_item_id + 1);
            let ident = format!("{}{}", IDENT_PREFIX, next_item_id);
            let objc_ident = NSString::alloc(nil).init_str(&ident);
            objc_ident as u64
        }
    }
    fn alloc_button(&mut self, image: Option<&str>, text: Option<&str>,
                    target: *mut Object, sel: SEL) -> *mut Object {
        unsafe {
            let text = match text {
                Some(s) => NSString::alloc(nil).init_str(s),
                None => nil,
            };
            let image = match image {
                Some(i) => {
                    let filename = NSString::alloc(nil).init_str(i);
                    let objc_image = NSImage::alloc(nil).initWithContentsOfFile_(filename);
                    let _ = msg_send![filename, release];
                    objc_image
                },
                None => nil,
            };
            let cls = Class::get("NSButton").unwrap();
            let btn: *mut Object;
            // Match on (image, text) as booleans.   false == null.
            match ((image as u64) != 0, (text as u64) != 0) {
                (false,true) => {
                    btn = msg_send![cls,
                                    buttonWithTitle: text
                                    target:target
                                    action:sel];
                }
                (true,false) => {
                    btn = msg_send![cls,
                                    buttonWithImage: image
                                    target:target
                                    action:sel];
                }
                (true,true) => {
                    btn = msg_send![cls,
                                    buttonWithTitle: text
                                    image:image
                                    target:target
                                    action:sel];
                }
                _ => { return nil }
            }
            btn
        }
    }
}

impl TouchbarTrait for Touchbar {
    type T = Touchbar;
    fn alloc(title: &str) -> Touchbar {
        let objc = ObjcAppDelegate::new().share();
        let rust = Box::new(RustTouchbarDelegateWrapper {
            objc: objc.clone(),
            next_item_id: Cell::new(0),
            bar_obj_map: BTreeMap::<ItemId, Ident>::new(),
            control_obj_map: BTreeMap::<ItemId, ItemId>::new(),
            scrubber_obj_map: BTreeMap::<ItemId, Scrubber>::new(),
            button_cb_map: BTreeMap::<ItemId, ButtonCb>::new(),
            slider_cb_map: BTreeMap::<ItemId, SliderCb>::new(),
        });
        unsafe {
            let ptr: u64 = &*rust as *const RustTouchbarDelegateWrapper as u64;
            let _ = msg_send![rust.objc, setRustWrapper: ptr];
            let objc_title = NSString::alloc(nil).init_str(title);
            let _ = msg_send![rust.objc, setTitle: objc_title];
        }
        return rust
    }
    fn set_icon(&self, image: &str) {
        unsafe {
            let filename = NSString::alloc(nil).init_str(image);
            let objc_image = NSImage::alloc(nil).initWithContentsOfFile_(filename);
            let _:() = msg_send![self.objc, setIcon: objc_image];
            let _ = msg_send![filename, release];
            let _ = msg_send![objc_image, release];
        }
    }
    fn enable(&self) {
        unsafe {
            let app = NSApp();
            let _: () = msg_send![app, setDelegate: self.objc.clone()];
            //let _: () = msg_send![self.objc, applicationDidFinishLaunching: 0];
        }
    }

//    pub fn add_button() {}
//    pub fn add_quit_button() {}
//    pub fn add_label() {}
    //    pub fn add_slider() {}
    fn create_bar(&mut self) -> BarId {
        unsafe {
            let ident = self.generate_ident();
            // Create touchbar
            let cls = Class::get("NSTouchBar").unwrap();
            let bar: *mut Object = msg_send![cls, alloc];
            let bar: *mut objc::runtime::Object = msg_send![bar, init];
            let _ : () = msg_send![bar, retain];
            let _ : () = msg_send![bar, setDelegate: self.objc.clone()];
            // Save tuple
            self.bar_obj_map.insert(bar as u64, ident as u64);
            bar as u64
        }
    }
    fn create_popover_item(&mut self, image: Option<&str>,
                           text: Option<&str>, bar_id: BarId) -> ItemId {
        unsafe {
            let bar = bar_id as *mut Object;
            let ident = self.generate_ident();
            let cls = Class::get("NSPopoverTouchBarItem").unwrap();
            let item: *mut Object = msg_send![cls, alloc];
            let item: *mut Object = msg_send![item, initWithIdentifier: ident];

            let target = (&*self.objc.clone()) as *const ObjcAppDelegate as *mut Object;
            let btn = self.alloc_button(image, text,
                                        target,
                                        sel!(popbar:));
            let _:() = msg_send![item, setShowsCloseButton: YES];
            let gesture: *mut Object = msg_send![item, makeStandardActivatePopoverGestureRecognizer];
            let _:() = msg_send![btn, addGestureRecognizer: gesture];
            let _:() = msg_send![item, setCollapsedRepresentation: btn];
            let _:() = msg_send![item, setPopoverTouchBar: bar];
            let _:() = msg_send![item, setPressAndHoldTouchBar: bar];

            self.bar_obj_map.insert(item as u64, ident as u64);
            self.control_obj_map.insert(btn as u64, item as u64);
            item as u64
        }
    }
    fn add_items_to_bar(&mut self, bar_id: BarId, items: Vec<ItemId>) {
        unsafe {
            let cls = Class::get("NSMutableArray").unwrap();
            let idents: *mut Object = msg_send![cls, arrayWithCapacity: items.len()];
            for item in items {
                let ident = *self.bar_obj_map.get(&item).unwrap() as *mut Object;
                let _ : () = msg_send![idents, addObject: ident];
            }
            let bar = bar_id as *mut Object;
            let _ : () = msg_send![bar, setDefaultItemIdentifiers: idents];
        }
    }
    fn set_bar_as_root(&mut self, bar_id: BarId) {
        unsafe {
            let old_bar: *mut Object = msg_send![self.objc, groupTouchBar];
            if old_bar != nil {
                // TODO: store in temp place until it's not visible,
                // delete and replace when it is closed.  otherwise
                // it forces the bar to close on each update.
                let visible: bool = msg_send![old_bar, isVisible];
                info!("DELETING OLD BAR: {}", visible);
                let cls = Class::get("NSTouchBar").unwrap();
                msg_send![cls, dismissSystemModalFunctionBar: old_bar];
            }
            let _ : () = msg_send![self.objc, setGroupTouchBar: bar_id];
            let ident: *mut Object = *self.bar_obj_map.get(&bar_id).unwrap() as *mut Object;
            let _ : () = msg_send![self.objc, setGroupIdent: ident];
            let _: () = msg_send![self.objc, applicationDidFinishLaunching: 0];
        }
    }
    fn create_label(&mut self, text: &str) -> ItemId {
        unsafe {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(300., 44.));
            let cls = Class::get("NSTextField").unwrap();
            let label: *mut Object = msg_send![cls, alloc];
            let label: *mut Object = msg_send![label, initWithFrame: frame];
            let _:() = msg_send![label, setEditable: NO];
            let text = NSString::alloc(nil).init_str(text);
            let _:() = msg_send![label, setStringValue: text];

            let ident = self.generate_ident();
            let cls = Class::get("NSCustomTouchBarItem").unwrap();
            let item: *mut Object = msg_send![cls, alloc];
            let item: *mut Object = msg_send![item, initWithIdentifier: ident];
            msg_send![item, setView: label];

            self.bar_obj_map.insert(item as u64, ident as u64);
            self.control_obj_map.insert(label as u64, item as u64);
            item as u64
        }
    }
    fn update_label(&mut self, label_id: ItemId, text: &str) {
        unsafe {
            let item: *mut Object = label_id as *mut Object;
            let label: *mut Object = msg_send![item, view];
            let text = NSString::alloc(nil).init_str(text);
            let _:() = msg_send![label, setStringValue: text];
        }
    }
    fn create_text_scrubber(&mut self, data: Rc<TScrubberData>) -> ItemId {
        unsafe {
            let ident = self.generate_ident();
            let cls = Class::get("NSCustomTouchBarItem").unwrap();
            let item: *mut Object = msg_send![cls, alloc];
            let item: *mut Object = msg_send![item, initWithIdentifier: ident];

            // note: frame is ignored, but must be provided.
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 30.));
            let cls = Class::get("NSScrubber").unwrap();
            let scrubber: *mut Object = msg_send![cls, alloc];
            let scrubber: *mut Object = msg_send![scrubber, initWithFrame: frame];

            let cls = Class::get("NSScrubberSelectionStyle").unwrap();
            let style: *mut Object = msg_send![cls, outlineOverlayStyle];

            let cls = Class::get("NSScrubberTextItemView").unwrap();
            let _:() = msg_send![scrubber, registerClass: cls forItemIdentifier: ident];
            let _:() = msg_send![scrubber, setDelegate: self.objc.clone()];
            let _:() = msg_send![scrubber, setDataSource: self.objc.clone()];
            let _:() = msg_send![scrubber, setSelectionOverlayStyle: style];
            let _:() = msg_send![scrubber, setMode: 1]; // NSScrubberModeFree
            //(*scrubber).set_ivar("selectedIndex", 3);
            //let _:() = msg_send![scrubber, ];
            let _:() = msg_send![item, setView: scrubber];

            self.bar_obj_map.insert(item as u64, ident as u64);
            let scrub_struct = Scrubber {
                data: data,
                _ident: ident as u64,
                _item: item as u64,
                _scrubber: scrubber as u64,
            };
            self.scrubber_obj_map.insert(scrubber as u64, scrub_struct);
            item as u64
        }
    }
    fn select_scrubber_item(&mut self, scrub_id: ItemId, index: u32) {
        unsafe {
            let item = scrub_id as *mut Object;
            let scrubber: *mut Object = msg_send![item, view];
            let _:() = msg_send![scrubber, setSelectedIndex: index];
        }
    }
    fn refresh_scrubber(&mut self, scrub_id: ItemId) {
        unsafe {
            let item = scrub_id as *mut Object;
            let scrubber: *mut Object = msg_send![item, view];
            let sel_idx: u32 = msg_send![scrubber, selectedIndex];
            let _:() = msg_send![scrubber, reloadData];
            // reload clears the selected item.  re-select it.
            let _:() = msg_send![scrubber, setSelectedIndex: sel_idx];
        }
    }
    fn create_button(&mut self, image: Option<&str>, text: Option<&str>, cb: ButtonCb) -> ItemId {
        unsafe {
            let ident = self.generate_ident();
            let target = (&*self.objc.clone()) as *const ObjcAppDelegate as *mut Object;
            let btn = self.alloc_button(image, text,
                                        target,
                                        sel!(button:));
            let cls = Class::get("NSCustomTouchBarItem").unwrap();
            let item: *mut Object = msg_send![cls, alloc];
            let item: *mut Object = msg_send![item, initWithIdentifier: ident];
            msg_send![item, setView: btn];

            self.bar_obj_map.insert(item as u64, ident as u64);
            self.control_obj_map.insert(btn as u64, item as u64);
            self.button_cb_map.insert(btn as u64, cb);
            item as u64
        }
    }
    fn create_slider(&mut self, min: f64, max: f64, cb: SliderCb) -> ItemId {
        unsafe {
            let ident = self.generate_ident();
            let cls = Class::get("NSSliderTouchBarItem").unwrap();
            let item: *mut Object = msg_send![cls, alloc];
            let item: *mut Object = msg_send![item, initWithIdentifier: ident];
            let slider: *mut Object = msg_send![item, slider];
            msg_send![slider, setMinValue: min];
            msg_send![slider, setMaxValue: max];
            msg_send![slider, setContinuous: YES];
            msg_send![item, setTarget: self.objc.clone()];
            msg_send![item, setAction: sel!(slider:)];
            self.bar_obj_map.insert(item as u64, ident as u64);
            self.control_obj_map.insert(slider as u64, item as u64);
            self.slider_cb_map.insert(slider as u64, cb);
            item as u64
        }
    }
    fn update_slider(&mut self, id: ItemId, value: f64) {
        unsafe {
            let item = id as *mut Object;
            let slider: *mut Object = msg_send![item, slider];
            let _:() = msg_send![slider, setDoubleValue: value];
        }
    }
}

// Below here defines a new native Obj-C class.
//
// See rustc-objc-foundation project by SSheldon, examples/custom_class.rs
// https://github.com/SSheldon/rust-objc-foundation/blob/master/examples/custom_class.rs
pub enum ObjcAppDelegate {}
impl ObjcAppDelegate {}

unsafe impl Message for ObjcAppDelegate { }

static OBJC_SUBCLASS_REGISTER_CLASS: Once = ONCE_INIT;

impl INSObject for ObjcAppDelegate {
    fn class() -> &'static Class {
        OBJC_SUBCLASS_REGISTER_CLASS.call_once(|| {
            let superclass = NSObject::class();
            let mut decl = ClassDecl::new("ObjcAppDelegate", superclass).unwrap();
            decl.add_ivar::<u64>("_rust_wrapper");
            decl.add_ivar::<u64>("_groupbar");
            decl.add_ivar::<u64>("_groupId");
            decl.add_ivar::<u64>("_title");
            decl.add_ivar::<u64>("_icon");

            extern fn objc_set_title(this: &mut Object, _cmd: Sel, ptr: u64) {
                unsafe {this.set_ivar("_title", ptr);}
            }
            extern fn objc_set_rust_wrapper(this: &mut Object, _cmd: Sel, ptr: u64) {
                unsafe {this.set_ivar("_rust_wrapper", ptr);}
            }
            extern fn objc_group_touch_bar(this: &mut Object, _cmd: Sel) -> u64 {
                unsafe {*this.get_ivar("_groupbar")}
            }
            extern fn objc_set_group_touch_bar(this: &mut Object, _cmd: Sel, bar: u64) {
                unsafe {this.set_ivar("_groupbar", bar);}
            }
            extern fn objc_set_group_ident(this: &mut Object, _cmd: Sel, bar: u64) {
                unsafe {this.set_ivar("_groupId", bar);}
            }
            extern fn objc_set_icon(this: &mut Object, _cmd: Sel, icon: u64) {
                unsafe {this.set_ivar("_icon", icon);}
            }
            extern fn objc_number_of_items_for_scrubber(this: &mut Object, _cmd: Sel,
                                                        scrub: u64) -> u32 {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let scrub_struct = wrapper.scrubber_obj_map.get(&scrub).unwrap();
                    let item = scrub_struct._item;
                    scrub_struct.data.count(item)
                }
            }
            extern fn objc_scrubber_view_for_item_at_index(this: &mut Object, _cmd: Sel,
                                                           scrub: u64, idx: u32) -> u64 {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let scrubber = scrub as *mut Object;
                    let scrub_struct = wrapper.scrubber_obj_map.get(&scrub).unwrap();
                    let item = scrub_struct._item;
                    let ident = scrub_struct._ident as *mut Object;
                    let view: *mut Object = msg_send![scrubber,
                                                      makeItemWithIdentifier:ident owner:nil];
                    let text = scrub_struct.data.text(item, idx);
                    let text_field: *mut Object = msg_send![view, textField];
                    let objc_text: *mut Object = NSString::alloc(nil).init_str(&text);
                    let _:() = msg_send![text_field, setStringValue: objc_text];
                    view as u64
                }
            }
            extern fn objc_scrubber_layout_size_for_item_at_index(this: &mut Object, _cmd: Sel,
                                                                  scrub: u64,
                                                                  _layout: u64, idx: u32) -> NSSize {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let scrub_struct = wrapper.scrubber_obj_map.get(&scrub).unwrap();
                    let item = scrub_struct._item;
                    let width = scrub_struct.data.width(item, idx);
                    NSSize::new(width as f64, 30.)
                }
            }
            extern fn objc_scrubber_did_select_item_at_index(this: &mut Object, _cmd: Sel,
                                                             scrub: u64, idx: u32) {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let scrub_struct = wrapper.scrubber_obj_map.get(&scrub).unwrap();
                    let item = scrub_struct._item;
                    scrub_struct.data.touch(item, idx);
                }
            }
            extern fn objc_popbar(this: &mut Object, _cmd: Sel, sender: u64) {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);

                    let item = *wrapper.control_obj_map.get(&sender).unwrap() as *mut Object;
                    let bar: *mut Object = msg_send![item, popoverTouchBar];
                    let ident = *wrapper.bar_obj_map.get(&(bar as u64)).unwrap() as *mut Object;

                    // Present the request popover.  This must be done instead of
                    // using the popover's built-in showPopover because that pops
                    // _under_ a system function bar.
                    let cls = Class::get("NSTouchBar").unwrap();
                    msg_send![cls,
                              presentSystemModalFunctionBar: bar
                              systemTrayItemIdentifier: ident];
                    let app = NSApp();
                    let _:() = msg_send![app, setTouchBar: nil];
                }
            }
            extern fn objc_button(this: &mut Object, _cmd: Sel, sender: u64) {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let ref cb = *wrapper.button_cb_map.get(&sender).unwrap();
                    cb(sender);
                }
            }
            extern fn objc_slider(this: &mut Object, _cmd: Sel, sender: u64) {
                unsafe {
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    let item = sender as *mut Object;
                    let slider: *mut Object = msg_send![item, slider];
                    let ref cb = *wrapper.slider_cb_map.get(&(slider as u64)).unwrap();
                    let value: f64 = msg_send![slider, doubleValue];
                    cb(sender, value);
                }
            }
            extern fn objc_present(this: &mut Object, _cmd: Sel, _sender: u64) {
                unsafe {
                    let ident_int: u64 = *this.get_ivar("_groupId");
                    let bar_int: u64 = *this.get_ivar("_groupbar");
                    let ident = ident_int as *mut Object;
                    let bar = bar_int as *mut Object;
                    let cls = Class::get("NSTouchBar").unwrap();
                    msg_send![cls,
                              presentSystemModalFunctionBar: bar
                              systemTrayItemIdentifier: ident];
                }
            }
            extern fn objc_touch_bar_make_item_for_identifier(this: &mut Object, _cmd: Sel,
                                                              _bar: u64, id_ptr: u64) -> u64 {
                unsafe {
                    // Find the touchbar item matching this identifier in the
                    // Objective-C object map of the Rust wrapper class, and
                    // return it if found.
                    let id = id_ptr as *mut Object;
                    let ptr: u64 = *this.get_ivar("_rust_wrapper");
                    let wrapper = &mut *(ptr as *mut RustTouchbarDelegateWrapper);
                    for (obj_ref, ident_ref) in &wrapper.bar_obj_map {
                        let ident = *ident_ref as *mut Object;
                        let obj = *obj_ref as *mut Object;
                        if msg_send![id, isEqualToString: ident] {
                            return obj as u64;
                        }
                    }
                }
                0
            }
            extern fn objc_application_did_finish_launching(this: &mut Object, _cmd: Sel, _notification: u64) {
                unsafe {
                    DFRSystemModalShowsCloseBoxWhenFrontMost(YES);

                    let ident_int: u64 = *this.get_ivar("_groupId");
                    let ident = ident_int as *mut Object;
                    let cls = Class::get("NSCustomTouchBarItem").unwrap();
                    let item: *mut Object = msg_send![cls, alloc];
                    msg_send![item, initWithIdentifier:ident];

                    let cls = Class::get("NSButton").unwrap();
                    let icon_ptr: u64 = *this.get_ivar("_icon");
                    let title_ptr: u64 = *this.get_ivar("_title");
                    let btn: *mut Object;
                    if icon_ptr != (nil as u64) {
                        btn = msg_send![cls,
                                        buttonWithImage:icon_ptr
                                        target:this
                                        action:sel!(present:)];
                    }
                    else {
                        btn = msg_send![cls,
                                        buttonWithTitle:title_ptr
                                        target:this
                                        action:sel!(present:)];
                    }
                    msg_send![item, setView:btn];

                    let cls = Class::get("NSTouchBarItem").unwrap();
                    msg_send![cls, addSystemTrayItem: item];
                    DFRElementSetControlStripPresenceForIdentifier(ident, YES);
                    println!("made bar");
                }
            }

            unsafe {
                let f: extern fn(&mut Object, Sel, u64, u64) -> u64 = objc_touch_bar_make_item_for_identifier;
                decl.add_method(sel!(touchBar:makeItemForIdentifier:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_application_did_finish_launching;
                decl.add_method(sel!(applicationDidFinishLaunching:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_present;
                decl.add_method(sel!(present:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_button;
                decl.add_method(sel!(button:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_slider;
                decl.add_method(sel!(slider:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_popbar;
                decl.add_method(sel!(popbar:), f);

                let f: extern fn(&mut Object, Sel) -> u64 = objc_group_touch_bar;
                decl.add_method(sel!(groupTouchBar), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_set_group_touch_bar;
                decl.add_method(sel!(setGroupTouchBar:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_set_group_ident;
                decl.add_method(sel!(setGroupIdent:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_set_icon;
                decl.add_method(sel!(setIcon:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_set_rust_wrapper;
                decl.add_method(sel!(setRustWrapper:), f);

                let f: extern fn(&mut Object, Sel, u64) = objc_set_title;
                decl.add_method(sel!(setTitle:), f);

                // Scrubber delegates
                let f: extern fn(&mut Object, Sel, u64) -> u32 = objc_number_of_items_for_scrubber;
                decl.add_method(sel!(numberOfItemsForScrubber:), f);
                let f: extern fn(&mut Object, Sel, u64, u32) -> u64 = objc_scrubber_view_for_item_at_index;
                decl.add_method(sel!(scrubber:viewForItemAtIndex:), f);
                let f: extern fn(&mut Object, Sel, u64, u64, u32) -> NSSize = objc_scrubber_layout_size_for_item_at_index;
                decl.add_method(sel!(scrubber:layout:sizeForItemAtIndex:), f);
                let f: extern fn(&mut Object, Sel, u64, u32) = objc_scrubber_did_select_item_at_index;
                decl.add_method(sel!(scrubber:didSelectItemAtIndex:), f);
            }

            decl.register();
        });

        Class::get("ObjcAppDelegate").unwrap()
    }
}