use crate::state::ProcessState;

pub fn check_step_limit(state: &ProcessState) -> Result<(), String> {
    if state.step > state.limits.max_steps {
        Err(format!("超出最大步数 {}", state.limits.max_steps))
    } else {
        Ok(())
    }
}

pub fn check_loop_limit(state: &ProcessState) -> Result<(), String> {
    if state.loop_count > state.limits.max_loop {
        Err(format!("超出最大环回次数 {}", state.limits.max_loop))
    } else {
        Ok(())
    }
}

pub fn check_retry_limit(state: &ProcessState) -> Result<(), String> {
    if state.retry_count > state.limits.max_retry {
        Err(format!("超出最大重试次数 {}", state.limits.max_retry))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Limits, ProcessState, Status};

    fn state_with(step: usize, loop_count: usize, retry_count: usize) -> ProcessState {
        ProcessState {
            workflow: "w".into(), instance_id: "i".into(), initial_input: None,
            status: Status::Idle, current: "a".into(), current_name: "A".into(),
            current_invoke: "invoke-1".into(), step, loop_count, retry_count,
            last_node: None, last_invoke: None, completed: vec![],
            limits: Limits { max_steps: 100, max_loop: 10, max_retry: 2 },
        }
    }

    #[test]
    fn step_limit_boundary() {
        assert!(check_step_limit(&state_with(100, 0, 0)).is_ok());
        assert!(check_step_limit(&state_with(101, 0, 0)).is_err());
    }

    #[test]
    fn loop_limit_boundary() {
        assert!(check_loop_limit(&state_with(0, 10, 0)).is_ok());
        assert!(check_loop_limit(&state_with(0, 11, 0)).is_err());
    }

    #[test]
    fn retry_limit_boundary() {
        assert!(check_retry_limit(&state_with(0, 0, 2)).is_ok());
        assert!(check_retry_limit(&state_with(0, 0, 3)).is_err());
    }
}
