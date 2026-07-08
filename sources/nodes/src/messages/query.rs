use serde::{Deserialize, Serialize};
use utils::AsAny;

use crate::pool::NodeUid;

pub struct Query {
    pub dest: NodeUid,
    pub body: Box<dyn QueryType>,
}

pub trait QueryType: AsAny {}

#[macro_export]
macro_rules! defquery {
    // Unit struct
    ($type_name:ident -> requests $req_purpose:literal and returns $return_type:ty) => {
        defquery!(@build $type_name $req_purpose $return_type | ;);
    };

    // With fields
    ($type_name:ident { $($f_name:ident: $f_type:ty),* $(,)? } requests $req_purpose:literal returns $return_type:ty) => {
        defquery!(@build $type_name $req_purpose $return_type | { $($f_name: $f_type),* });
    };

    // Shared expansion
    (@build $type_name:ident $req_purpose:literal $return_type:ty | $body:tt) => {
        #[doc = concat!("A query that requests ", $req_purpose,
            ".\n\n## Returns\n[`", stringify!($return_type), "`] if the query succeeds.")]
        #[derive(Clone, Serialize, Deserialize)]
        pub struct $type_name $body

        impl $crate::messages::QueryType for $type_name {}
    };
}

defquery! { Size -> requests "the last known size of a node" and returns egui::Vec2 }
