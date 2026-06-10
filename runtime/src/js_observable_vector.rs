use std::cell::{Cell, RefCell};
use windows::Foundation::Collections::{
    CollectionChange, IObservableVector, IObservableVector_Impl, IVectorChangedEventArgs,
    IVectorChangedEventArgs_Impl, VectorChangedEventHandler,
};
use windows::Foundation::PropertyValue;
use windows_collections::{
    IIterable, IIterable_Impl, IIterator, IIterator_Impl, IVector, IVectorView, IVectorView_Impl,
    IVector_Impl,
};
use windows_core::{implement, IInspectable, Interface, Ref, Result, HRESULT};

const E_BOUNDS: HRESULT = HRESULT(0x8000_000Bu32 as i32);

fn err_bounds() -> windows_core::Error {
    windows_core::Error::from_hresult(E_BOUNDS)
}

#[implement(IIterator<IInspectable>)]
struct JsVectorIterator {
    items: Vec<IInspectable>,
    pos: RefCell<usize>,
}

impl IIterator_Impl<IInspectable> for JsVectorIterator_Impl {
    fn Current(&self) -> Result<IInspectable> {
        let pos = *self.pos.borrow();
        self.items.get(pos).cloned().ok_or_else(err_bounds)
    }
    fn HasCurrent(&self) -> Result<bool> {
        Ok(*self.pos.borrow() < self.items.len())
    }
    fn MoveNext(&self) -> Result<bool> {
        let mut pos = self.pos.borrow_mut();
        if *pos < self.items.len() {
            *pos += 1;
        }
        Ok(*pos < self.items.len())
    }
    fn GetMany(&self, items: &mut [Option<IInspectable>]) -> Result<u32> {
        let mut pos = self.pos.borrow_mut();
        let mut n = 0usize;
        while n < items.len() && *pos < self.items.len() {
            items[n] = Some(self.items[*pos].clone());
            *pos += 1;
            n += 1;
        }
        Ok(n as u32)
    }
}

#[implement(IVectorView<IInspectable>)]
struct JsVectorView {
    items: Vec<IInspectable>,
}

impl IIterable_Impl<IInspectable> for JsVectorView_Impl {
    fn First(&self) -> Result<IIterator<IInspectable>> {
        Ok(JsVectorIterator {
            items: self.items.clone(),
            pos: RefCell::new(0),
        }
        .into())
    }
}

impl IVectorView_Impl<IInspectable> for JsVectorView_Impl {
    fn GetAt(&self, index: u32) -> Result<IInspectable> {
        self.items
            .get(index as usize)
            .cloned()
            .ok_or_else(err_bounds)
    }
    fn Size(&self) -> Result<u32> {
        Ok(self.items.len() as u32)
    }
    fn IndexOf(&self, value: Ref<IInspectable>, index: &mut u32) -> Result<bool> {
        *index = 0;
        let target = match value.as_ref() {
            Some(v) => v,
            None => return Ok(false),
        };
        for (i, it) in self.items.iter().enumerate() {
            if it.as_raw() == target.as_raw() {
                *index = i as u32;
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn GetMany(&self, start_index: u32, items: &mut [Option<IInspectable>]) -> Result<u32> {
        let start = start_index as usize;
        let mut n = 0usize;
        while n < items.len() && start + n < self.items.len() {
            items[n] = Some(self.items[start + n].clone());
            n += 1;
        }
        Ok(n as u32)
    }
}

#[implement(IVectorChangedEventArgs)]
struct JsVectorChangedEventArgs {
    index: u32,
    change: CollectionChange,
}

impl IVectorChangedEventArgs_Impl for JsVectorChangedEventArgs_Impl {
    fn Index(&self) -> Result<u32> {
        Ok(self.index)
    }
    fn CollectionChange(&self) -> Result<CollectionChange> {
        Ok(self.change)
    }
}

#[implement(IObservableVector<IInspectable>, IVector<IInspectable>, IIterable<IInspectable>)]
struct JsVector {
    items: RefCell<Vec<IInspectable>>,
    handlers: RefCell<Vec<(i64, VectorChangedEventHandler<IInspectable>)>>,
    next_token: Cell<i64>,
}

impl IObservableVector_Impl<IInspectable> for JsVector_Impl {
    fn VectorChanged(&self, vhnd: Ref<VectorChangedEventHandler<IInspectable>>) -> Result<i64> {
        let token = self.next_token.get() + 1;
        self.next_token.set(token);
        if let Some(h) = vhnd.as_ref() {
            self.handlers.borrow_mut().push((token, h.clone()));
        }
        Ok(token)
    }
    fn RemoveVectorChanged(&self, token: i64) -> Result<()> {
        self.handlers.borrow_mut().retain(|(t, _)| *t != token);
        Ok(())
    }
}

impl JsVector_Impl {
    fn fire_vector_changed(&self, index: u32, change: CollectionChange) -> Result<()> {
        let args: IVectorChangedEventArgs = JsVectorChangedEventArgs { index, change }.into();
        let sender: Option<IObservableVector<IInspectable>> = None;
        let guard = self.handlers.borrow();
        for (_, h) in guard.iter() {
            h.Invoke(sender.as_ref(), Some(&args))?;
        }
        Ok(())
    }
}

impl IIterable_Impl<IInspectable> for JsVector_Impl {
    fn First(&self) -> Result<IIterator<IInspectable>> {
        Ok(JsVectorIterator {
            items: self.items.borrow().clone(),
            pos: RefCell::new(0),
        }
        .into())
    }
}

impl IVector_Impl<IInspectable> for JsVector_Impl {
    fn GetAt(&self, index: u32) -> Result<IInspectable> {
        self.items
            .borrow()
            .get(index as usize)
            .cloned()
            .ok_or_else(err_bounds)
    }
    fn Size(&self) -> Result<u32> {
        Ok(self.items.borrow().len() as u32)
    }
    fn GetView(&self) -> Result<IVectorView<IInspectable>> {
        Ok(JsVectorView {
            items: self.items.borrow().clone(),
        }
        .into())
    }
    fn IndexOf(&self, value: Ref<IInspectable>, index: &mut u32) -> Result<bool> {
        *index = 0;
        let target = match value.as_ref() {
            Some(v) => v,
            None => return Ok(false),
        };
        for (i, it) in self.items.borrow().iter().enumerate() {
            if it.as_raw() == target.as_raw() {
                *index = i as u32;
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn SetAt(&self, index: u32, value: Ref<IInspectable>) -> Result<()> {
        {
            let mut items = self.items.borrow_mut();
            if (index as usize) >= items.len() {
                return Err(err_bounds());
            }
            if let Some(v) = value.as_ref() {
                items[index as usize] = v.clone();
            }
        }
        self.fire_vector_changed(index, CollectionChange::ItemChanged)?;
        Ok(())
    }
    fn InsertAt(&self, index: u32, value: Ref<IInspectable>) -> Result<()> {
        {
            let mut items = self.items.borrow_mut();
            if (index as usize) > items.len() {
                return Err(err_bounds());
            }
            if let Some(v) = value.as_ref() {
                items.insert(index as usize, v.clone());
            }
        }
        self.fire_vector_changed(index, CollectionChange::ItemInserted)?;
        Ok(())
    }
    fn RemoveAt(&self, index: u32) -> Result<()> {
        {
            let mut items = self.items.borrow_mut();
            if (index as usize) >= items.len() {
                return Err(err_bounds());
            }
            items.remove(index as usize);
        }
        self.fire_vector_changed(index, CollectionChange::ItemRemoved)?;
        Ok(())
    }
    fn Append(&self, value: Ref<IInspectable>) -> Result<()> {
        let index = {
            let mut items = self.items.borrow_mut();
            let index = items.len() as u32;
            if let Some(v) = value.as_ref() {
                items.push(v.clone());
            }
            index
        };
        self.fire_vector_changed(index, CollectionChange::ItemInserted)?;
        Ok(())
    }
    fn RemoveAtEnd(&self) -> Result<()> {
        let index = {
            let mut items = self.items.borrow_mut();
            if items.is_empty() {
                return Err(err_bounds());
            }
            let index = (items.len() - 1) as u32;
            items.pop();
            index
        };
        self.fire_vector_changed(index, CollectionChange::ItemRemoved)?;
        Ok(())
    }
    fn Clear(&self) -> Result<()> {
        self.items.borrow_mut().clear();
        self.fire_vector_changed(0, CollectionChange::Reset)?;
        Ok(())
    }
    fn GetMany(&self, start_index: u32, items: &mut [Option<IInspectable>]) -> Result<u32> {
        let store = self.items.borrow();
        let start = start_index as usize;
        let mut n = 0usize;
        while n < items.len() && start + n < store.len() {
            items[n] = Some(store[start + n].clone());
            n += 1;
        }
        Ok(n as u32)
    }
    fn ReplaceAll(&self, values: &[Option<IInspectable>]) -> Result<()> {
        {
            let mut items = self.items.borrow_mut();
            items.clear();
            for v in values {
                if let Some(v) = v {
                    items.push(v.clone());
                }
            }
        }
        self.fire_vector_changed(0, CollectionChange::Reset)?;
        Ok(())
    }
}

pub(crate) fn make_index_vector(count: u32) -> Result<IInspectable> {
    let mut items: Vec<IInspectable> = Vec::with_capacity(count as usize);
    for i in 0..count {
        items.push(PropertyValue::CreateInt32(i as i32)?);
    }
    let vector: IObservableVector<IInspectable> = JsVector {
        items: RefCell::new(items),
        handlers: RefCell::new(Vec::new()),
        next_token: Cell::new(0),
    }
    .into();
    Ok(vector.cast()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use windows::Foundation::Collections::{
        CollectionChange, IObservableVector, IVectorChangedEventArgs, VectorChangedEventHandler,
    };
    use windows::Foundation::{IPropertyValue, PropertyValue};
    use windows_collections::{IIterable, IIterator, IVector, IVectorView};
    use windows_core::IInspectable as WInspectable;

    #[test]
    fn make_index_vector_values() -> Result<()> {
        let inspectable = make_index_vector(5)?;
        let vec: IVector<WInspectable> = inspectable.cast()?;
        assert_eq!(vec.Size()?, 5u32);
        for i in 0..5u32 {
            let item = vec.GetAt(i)?;
            let pv: IPropertyValue = item.cast()?;
            let v = pv.GetInt32()?;
            assert_eq!(v, i as i32);
        }
        Ok(())
    }

    #[test]
    fn iterator_works() -> Result<()> {
        let inspectable = make_index_vector(3)?;
        let iterable: IIterable<WInspectable> = inspectable.cast()?;
        let iter: IIterator<WInspectable> = iterable.First()?;
        let mut expected = 0i32;
        while iter.HasCurrent()? {
            let item = iter.Current()?;
            let pv: IPropertyValue = item.cast()?;
            assert_eq!(pv.GetInt32()?, expected);
            iter.MoveNext()?;
            expected += 1;
        }
        assert_eq!(expected, 3);
        Ok(())
    }

    #[test]
    fn view_is_snapshot() -> Result<()> {
        let inspectable = make_index_vector(3)?;
        let vec: IVector<WInspectable> = inspectable.cast()?;
        let view: IVectorView<WInspectable> = vec.GetView()?;

        // mutate the original vector — replace all values with a different value
        let new_val = PropertyValue::CreateInt32(999)?;
        let new_ins = new_val.cast::<WInspectable>()?;
        let replacements: Vec<Option<WInspectable>> =
            vec![Some(new_ins.clone()), Some(new_ins.clone()), Some(new_ins)];
        vec.ReplaceAll(&replacements)?;

        // the snapshot view should still reflect the original value (0)
        let item = view.GetAt(0)?;
        let pv: IPropertyValue = item.cast()?;
        assert_eq!(pv.GetInt32()?, 0);
        Ok(())
    }

    #[test]
    fn vector_changed_event_works() -> Result<()> {
        let inspectable = make_index_vector(3)?;
        let vec: IObservableVector<WInspectable> = inspectable.cast()?;

        let calls = Arc::new(Mutex::new(Vec::<(u32, CollectionChange)>::new()));
        let calls_clone = calls.clone();

        let handler = VectorChangedEventHandler::new(
            move |_sender: Ref<IObservableVector<WInspectable>>,
                  args: Ref<IVectorChangedEventArgs>| {
                if let Some(a) = args.as_ref() {
                    let idx = a.Index()?;
                    let ch = a.CollectionChange()?;
                    calls_clone.lock().unwrap().push((idx, ch));
                } else {
                    calls_clone
                        .lock()
                        .unwrap()
                        .push((0, CollectionChange::Reset));
                }
                Ok(())
            },
        );

        let token = vec.VectorChanged(&handler)?;

        // mutate: SetAt should trigger ItemChanged at index 1
        let pv = PropertyValue::CreateInt32(42)?;
        let ins = pv.cast::<WInspectable>()?;
        vec.SetAt(1, &ins)?;

        let locked = calls.lock().unwrap();
        assert_eq!(locked.len(), 1);
        let (idx, ch) = locked[0];
        assert_eq!(idx, 1u32);
        assert_eq!(ch, CollectionChange::ItemChanged);

        vec.RemoveVectorChanged(token)?;
        Ok(())
    }

    #[test]
    fn handler_can_reenter_during_notification() -> Result<()> {
        // Regression: mutators must release the items borrow before raising
        // VectorChanged so a handler (like XAML's ListView) can synchronously re-read
        // Size()/GetAt() from inside the notification without a RefCell double-borrow panic.
        let inspectable = make_index_vector(3)?;
        let vec: IObservableVector<WInspectable> = inspectable.cast()?;
        // A second interface pointer to the SAME object, captured by the handler so it
        // reenters the live collection during dispatch.
        // let vec_in_handler: IVector<WInspectable> = inspectable.cast()?;

        // Each entry: (index reported by args, Size() observed during the callback).
        let seen = Arc::new(Mutex::new(Vec::<(u32, u32)>::new()));
        let seen_clone = seen.clone();

        let handler = VectorChangedEventHandler::new(
            move |sender: Ref<IObservableVector<WInspectable>>,
                  args: Ref<IVectorChangedEventArgs>| {
                let vec: IVector<WInspectable> = sender.ok()?.cast()?;
                let size = vec.Size()?;
                if size > 0 {
                    // Reads the front element while the mutation that triggered us is done.
                    let _ = vec.GetAt(0)?;
                }
                let idx = match args.as_ref() {
                    Some(a) => a.Index()?,
                    None => 0,
                };
                seen_clone.lock().unwrap().push((idx, size));
                Ok(())
            },
        );

        let token = vec.VectorChanged(&handler)?;

        // Append → ItemInserted at index 3; size is now 4.
        let pv = PropertyValue::CreateInt32(7)?;
        let ins = pv.cast::<WInspectable>()?;
        vec.Append(&ins)?;

        // RemoveAt(0) → ItemRemoved at index 0; size is now 3.
        vec.RemoveAt(0)?;

        // SetAt(0) → ItemChanged at index 0; size unchanged at 3.
        let pv2 = PropertyValue::CreateInt32(99)?;
        let ins2 = pv2.cast::<WInspectable>()?;
        vec.SetAt(0, &ins2)?;

        let locked = seen.lock().unwrap();
        assert_eq!(*locked, vec![(3u32, 4u32), (0, 3), (0, 3)]);
        drop(locked);

        vec.RemoveVectorChanged(token)?;
        Ok(())
    }
}
