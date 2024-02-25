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
        $crate::args!(@arms ($($rest)*) ->
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
    (@arms ($help:literal $name:ident: $container:tt<$type:ty> []= $default:expr; $($rest:tt)*) ->
     ($($carry:tt)*) ($($sb:tt)*) ($($db:tt)*) ($($se:tt)*)) =>
    {
        $crate::args!(@arms ($($rest)*) ->
               ($($carry)*)
               ($($sb)*
                #[arg(long, num_args = 0.., default_values_t = $default, help = $help)]
                pub $name: $container<$type>,)
               ($($db)*
               $name: $default.into_iter().collect(),)
               ($($se)*
                pub fn $name(mut self, $name: impl IntoIterator<Item=$type>) -> Self {
                    self.$name = $name.into_iter().collect();
                    self
                })
        );
    };

    // A normal value
    (@arms ($help:literal $name:ident: $type:ty = $default:expr; $($rest:tt)*) ->
     ($($carry:tt)*) ($($sb:tt)*) ($($db:tt)*) ($($se:tt)*)) =>
    {
        $crate::args!(@arms ($($rest)*) ->
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
        $crate::args!(@arms ($($rest)*) -> ($(#[$meta])* $name) () () ());
    };
}

pub use args;

#[cfg(test)]
mod test {
    #![allow(dead_code)]

    use clap::Parser;

    use super::*;

    #[derive(clap::Args, Debug, PartialEq)]
    pub struct Manual {
        #[arg(long, default_value_t = 21, help = "hej")]
        yas: i32,
    }

    impl std::default::Default for Manual {
        fn default() -> Self {
            Self { yas: 22 }
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

    #[derive(Parser, Debug)]
    #[command()]
    struct Cmd {
        #[command(flatten)]
        auto: Auto,
    }

    #[test]
    fn default() {
        let auto = Auto::default();
        assert_eq!(21, auto.omg);
        assert_eq!(22, auto.asd.yas);
    }

    #[test]
    fn list() {
        let auto = Cmd::try_parse_from([""]).unwrap().auto;
        assert_eq!(vec![1, 2], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "1"]).unwrap().auto;
        assert_eq!(vec![1], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "1", "5"]).unwrap().auto;
        assert_eq!(vec![1, 5], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej"]).unwrap().auto;
        assert_eq!(Vec::<i32>::new(), auto.hej);

        assert!(Cmd::try_parse_from(["", "--hej", ""]).is_err());

        let auto = Cmd::try_parse_from(["", "--hej", "--yas", "78"])
            .unwrap()
            .auto;
        assert_eq!(Vec::<i32>::new(), auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "--hej", "78"])
            .unwrap()
            .auto;
        assert_eq!(vec![78], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "--hej", "78", "--hej", "12"])
            .unwrap()
            .auto;
        assert_eq!(vec![78, 12], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "78", "--hej", "12"])
            .unwrap()
            .auto;
        assert_eq!(vec![78, 12], auto.hej);

        let auto = Cmd::try_parse_from(["", "--hej", "78", "--hej", "12", "--hej"])
            .unwrap()
            .auto;
        assert_eq!(vec![78, 12], auto.hej);
    }
}
