#[cfg(test)]
mod tests {
    use crate::todos::{
        contract, todo_apply_agent_event, todo_create, todo_delete, todo_list, todo_submit_guide,
        todo_update,
    };

    #[test]
    fn todo_command_names_load_from_contract_manifest() {
        let commands = contract::commands();
        let _ = todo_list;
        let _ = todo_create;
        let _ = todo_submit_guide;
        let _ = todo_update;
        let _ = todo_delete;
        let _ = todo_apply_agent_event::<tauri::Wry>;

        assert_eq!(commands.list, stringify!(todo_list));
        assert_eq!(commands.create, stringify!(todo_create));
        assert_eq!(commands.submit_guide, stringify!(todo_submit_guide));
        assert_eq!(commands.update, stringify!(todo_update));
        assert_eq!(commands.delete, stringify!(todo_delete));
        assert_eq!(
            commands.apply_agent_event,
            stringify!(todo_apply_agent_event)
        );
    }
}
