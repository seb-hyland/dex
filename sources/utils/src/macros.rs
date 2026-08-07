#[macro_export]
/**
    Pattern match on `Box<dyn MyTrait>` where `MyTrait: AsAny`.
*/
macro_rules! match_dyn {
    (
        $matched_item:expr, $($binding:ident : $match_type:ty => $body:expr,)*
        _ => $rest:expr $(,)?
    ) => {
        match $matched_item {
            $($binding if (*$binding).as_any_ref().is::<$match_type>() => {
                let $binding = *$binding.as_any().downcast::<$match_type>().unwrap();
                $body
            })*
            _ => $rest
        }
    };
}

#[macro_export]
macro_rules! boxed_any {
    ($val:expr) => {
        ::std::boxed::Box::new($val) as ::std::boxed::Box<dyn ::std::any::Any>
    };
}
