#[macro_export]
macro_rules! children {
    ($($child:expr),* $(,)?) => {
        vec![$(Box::new($child) as Box<dyn $crate::widget::Widget>),*]
    };
}

#[macro_export]
macro_rules! mgui {
    ($container:expr => [ $($rest:tt)* ]) => {{
        mgui!(@build $container, $($rest)*)
    }};

    ($leaf:expr) => {
        $leaf
    };

    (@build $c:expr, $child:expr => [ $($inner:tt)* ], $($rest:tt)*) => {
        mgui!(@build $c.child(mgui!($child => [ $($inner)* ])), $($rest)*)
    };

    (@build $c:expr, $child:expr => [ $($inner:tt)* ]) => {
        $c.child(mgui!($child => [ $($inner)* ]))
    };

    (@build $c:expr, $child:expr, $($rest:tt)*) => {
        mgui!(@build $c.child($child), $($rest)*)
    };

    (@build $c:expr, $child:expr) => {
        $c.child($child)
    };

    (@build $c:expr,) => { $c };
    (@build $c:expr) => { $c };
}
