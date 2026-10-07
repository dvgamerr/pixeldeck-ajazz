use crate::{AjazzError, AjazzInput, DeviceState, Event};

/// Applies a raw device input to the tracked state and returns the resulting events.
pub(crate) fn handle_input_state_change(
    input: AjazzInput,
    current_state: &mut DeviceState,
) -> Result<Vec<Event>, AjazzError> {
    match input {
        AjazzInput::ButtonStateChange(buttons) => Ok(toggle_changed(
            &buttons,
            &mut current_state.buttons,
            Event::ButtonDown,
            Event::ButtonUp,
        )),
        AjazzInput::EncoderStateChange(encoders) => Ok(toggle_changed(
            &encoders,
            &mut current_state.encoders,
            Event::EncoderDown,
            Event::EncoderUp,
        )),
        AjazzInput::EncoderTwist(twist) => Ok(encoder_twists(&twist)),
        AjazzInput::ButtonEvent(index, pressed) => {
            button_event(index, pressed, &mut current_state.buttons)
        }
        AjazzInput::EncoderPulse(index) => encoder_pulse(index, current_state.encoders.len()),
        _ => Ok(vec![]),
    }
}

/// Flips every changed slot in `state` and emits a down/up event for its new value.
fn toggle_changed(
    changed: &[bool],
    state: &mut [bool],
    down: fn(u8) -> Event,
    up: fn(u8) -> Event,
) -> Vec<Event> {
    let mut updates = vec![];
    for (index, is_changed) in changed.iter().enumerate() {
        if !is_changed {
            continue;
        }

        state[index] = !state[index];
        updates.push(if state[index] {
            down(index as u8)
        } else {
            up(index as u8)
        });
    }
    updates
}

fn encoder_twists(twist: &[i8]) -> Vec<Event> {
    twist
        .iter()
        .enumerate()
        .filter(|(_, change)| **change != 0)
        .map(|(index, change)| Event::EncoderTwist(index as u8, *change))
        .collect()
}

fn button_event(
    index: u8,
    pressed: bool,
    buttons: &mut [bool],
) -> Result<Vec<Event>, AjazzError> {
    let Some(current) = buttons.get_mut(index as usize) else {
        return Err(AjazzError::BadData);
    };
    if *current == pressed {
        return Ok(vec![]);
    }

    *current = pressed;
    Ok(vec![if pressed {
        Event::ButtonDown(index)
    } else {
        Event::ButtonUp(index)
    }])
}

fn encoder_pulse(index: u8, encoder_count: usize) -> Result<Vec<Event>, AjazzError> {
    if index as usize >= encoder_count {
        return Err(AjazzError::BadData);
    }
    Ok(vec![Event::EncoderDown(index), Event::EncoderUp(index)])
}
