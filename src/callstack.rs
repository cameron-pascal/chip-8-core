#[derive(Debug, PartialEq)]
pub enum CallStackErr {
    StackOverflow,
    StackEmpty
}

pub struct CallStack {
    arr: Vec<u16>,
    top: i16,
}

impl CallStack {
    pub fn new(size: usize) -> CallStack {
        CallStack {
            arr: vec![0; size],
            top: -1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.top == -1
    }

    pub fn is_full(&self) -> bool {
        self.top + 1 == self.arr.len() as i16
    }

    pub fn push(&mut self, addr: u16) -> Result<(), CallStackErr> {
        if self.is_full() {
            return Err(CallStackErr::StackOverflow);
        }

        self.top += 1;
        self.arr[self.top as usize] = addr;

        Ok(())
    }    

    pub fn pop(&mut self) -> Result<u16, CallStackErr> {
        if self.is_empty() {
            return Err(CallStackErr::StackEmpty);
        }

        let instr = self.arr[self.top as usize];
        self.top -= 1;

        Ok(instr)
    }

    pub fn snapshot(&self) -> Option<Vec<u16>> {
        if self.is_empty() {
            return None
        }

        let mut snapshot: Vec<u16> = vec![];

        for idx in 0..=self.top {
            snapshot.push(self.arr[idx as usize]);
        }

        Some(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_test() {
        let mut call_stack = CallStack::new(12);
        call_stack.push(1).unwrap();
        call_stack.push(2).unwrap();

        let val = call_stack.pop().unwrap();
        assert_eq!(2, val);

        let val = call_stack.pop().unwrap();
        assert_eq!(1, val);

        let result = call_stack.pop();
        match result {
            Err(CallStackErr::StackEmpty) => assert!(true),
            _ => assert!(false),
        }

        for _i in 0..12 {
            call_stack.push(1).unwrap();
        }

        let result = call_stack.push(1);
        match result {
            Err(CallStackErr::StackOverflow) => assert!(true),
            _ => assert!(false),
        }
    }
}