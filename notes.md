Text should emit on focus loss + change with caching
Probably something that allows customization of emit command
```rust
fn show(
    &self, 
    ui: &mut Ui, 
    change_handler: impl FnMut(RopeBuffer) -> CanvasAction, 
    show_fn: impl FnMut(&mut Ui, &mut RopeBuffer) -> Response
) -> Option<CanvasAction>;
```
with options `show_singleline`, `show_code_editor`
