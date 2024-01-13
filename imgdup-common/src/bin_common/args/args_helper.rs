#[macro_export]
macro_rules! args {
    // The end
    (@arms () -> ($(#[$meta:meta])* $name:ident) ($($structbody:tt)*) ($($defaultbody:tt)*) ($($setters:tt)*)) =>
    {
        #[derive(Debug, clap::Args)]
        $(#[$meta])*
        pub struct $name {
            $($structbody)*
        }

        impl std::default::Default for $name {
            fn default() -> Self {
                Self {
                    $($defaultbody)*
                }
            }
        }

        impl $name {
            $($setters)*
        }
    };

    // Flattening
    (@arms ($name:ident: $type:ty; $($rest:tt)*) ->
     ($($carry:tt)*) ($($sb:tt)*) ($($db:tt)*) ($($se:tt)*)) =>
    {
        args!(@arms ($($rest)*) ->
               ($($carry)*)
               ($($sb)*
                #[command(flatten)]
                pub $name: $type,)
               ($($db)*
               $name: <$type>::default(),)
               ($($se)*
                pub fn $name(mut self, $name: $type) -> Self {
                    self.$name = $name;
                    self
                }));
    };

    // A list
    (@arms ($help:literal $name:ident: $type:ty []= $default:expr; $($rest:tt)*) ->
     ($($carry:tt)*) ($($sb:tt)*) ($($db:tt)*) ($($se:tt)*)) =>
    {
        args!(@arms ($($rest)*) ->
               ($($carry)*)
               ($($sb)*
                #[arg(long, default_values_t = $default, help = $help)]
                pub $name: $type,)
               ($($db)*
               $name: $default.into_iter().collect(),)
               ($($se)*
                pub fn $name(mut self, $name: $type) -> Self {
                    self.$name = $name;
                    self
                })
        );
    };

    // A normal value
    (@arms ($help:literal $name:ident: $type:ty = $default:expr; $($rest:tt)*) ->
     ($($carry:tt)*) ($($sb:tt)*) ($($db:tt)*) ($($se:tt)*)) =>
    {
        args!(@arms ($($rest)*) ->
               ($($carry)*)
               ($($sb)*
                #[arg(long, default_value_t = $default, help = $help)]
                pub $name: $type,)
               ($($db)*
                $name: $default,)
               ($($se)*
                pub fn $name(mut self, $name: $type) -> Self {
                    self.$name = $name;
                    self
                }));
    };

    // Start here
    ($(#[$meta:meta])* $name:ident {$($rest:tt)*}) =>
    {
        args!(@arms ($($rest)*) -> ($(#[$meta])* $name) () () ());
    };
}

pub use args;

#[cfg(test)]
mod test {
    #![allow(dead_code)]

    #[derive(clap::Args, Debug, PartialEq)]
    pub struct Manual {
        #[arg(long, default_value_t = 21, help = "hej")]
        omg: i32,
    }

    impl std::default::Default for Manual {
        fn default() -> Self {
            Self { omg: 22 }
        }
    }

    args! {
        #[derive(PartialEq)]
        Auto {
            "omg"
            omg: i32 = 21;

            "hej"
            hej: Vec<i32> []= [1, 2];

            asd: Manual;
        }
    }

    #[test]
    fn args() {
        let auto = Auto::default();
        assert_eq!(21, auto.omg);
        assert_eq!(22, auto.asd.omg);
    }
}
