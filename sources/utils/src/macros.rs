#[macro_export]
/**
    Pattern match on `Box<dyn MyTrait>` where `MyTrait: AsAny`.
*/
macro_rules! match_dyn {
    (
        $matcher:expr, $($binding:ident : $match_type:ty => $body:expr,)*
        _ => $rest:expr $(,)?
    ) => {
        match $matcher {
            $($binding if $binding.as_any_ref().is::<$match_type>() => {
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
        Box::new($val) as Box<dyn ::std::any::Any>
    };
}
