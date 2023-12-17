macro_rules! args {
    ($(#$argsmeta:tt)* $name:ident { $($fhelp:literal $fname:ident: $ftype:ty = $fdefault:expr);+ $(;)* }) => {
        paste::paste! {
            #[derive(clap::Args, Debug)]
            pub struct [<$name Cli>] {
                $(
                    #[arg(long, default_value_t = ($fdefault), help = $fhelp)]
                    $fname: $ftype,
                )+
            }

            $(#$argsmeta)*
            pub struct [<$name Args>] {
                $(
                    $fname: $ftype,
                )+
            }

            impl std::default::Default for [<$name Args>] {
                fn default() -> Self {
                    Self {
                        $(
                            $fname: $fdefault,
                        )+
                    }
                }
            }

            impl [<$name Args>] {
                $(
                    pub fn $fname(mut self, $fname: $ftype) -> Self {
                        self.$fname = $fname;
                        self
                    }
                )+
            }

            impl [<$name Cli>] {
                pub fn to_args(&self) -> [<$name Args>] {
                    [<$name Args>] {
                        $(
                            $fname: self.$fname.clone(),
                        )+
                    }
                }
            }
        }
    };
}

pub(crate) use args;
