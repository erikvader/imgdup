#[macro_export]
macro_rules! args {
    ($(#$argsmeta:tt)* $name:ident {
        $($fhelp:literal $fname:ident: $ftype:ty = $fdefault:expr;)*
        $([] $vhelp:literal $vname:ident: $vtype:ty = $vdefault:expr;)*
        $($mname:ident: $mtype:ty;)*
    }) => {
        $crate::bin_common::args::args_helper::paste! {
            #[derive(clap::Args, Debug)]
            pub struct [<$name Cli>] {
                $(
                    #[arg(long, default_value_t = ($fdefault), help = $fhelp)]
                    $fname: $ftype,
                )*

                // TODO: a nicer way to solve this than to copy paste the other case with
                // ONE character difference?
                $(
                    #[arg(long, default_values_t = ($vdefault), help = $vhelp)]
                    $vname: $vtype,
                )*

                $(
                    #[command(flatten)]
                    $mname: [<$mtype Cli>],
                )*
            }

            // TODO: why even creating a struct that looks exactly the same? Whats the
            // difference between XXXCli and XXXArgs? Was there ever even one? Tankevurpa
            // from the beginning?
            $(#$argsmeta)*
            pub struct [<$name Args>] {
                $(
                    $fname: $ftype,
                )*

                $(
                    $vname: $vtype,
                )*

                $(
                    $mname: [<$mtype Args>],
                )*
            }

            impl std::default::Default for [<$name Args>] {
                fn default() -> Self {
                    Self {
                        $(
                            $fname: $fdefault,
                        )*

                        $(
                            $vname: $vdefault.into_iter().collect(),
                        )*

                        $(
                            $mname: [<$mtype Args>]::default(),
                        )*
                    }
                }
            }

            impl [<$name Args>] {
                $(
                    pub fn $fname(mut self, $fname: $ftype) -> Self {
                        self.$fname = $fname;
                        self
                    }
                )*

                $(
                    pub fn $vname(mut self, $vname: $vtype) -> Self {
                        self.$vname = $vname;
                        self
                    }
                )*

                $(
                    pub fn $mname(mut self, $mname: [<$mtype Args>]) -> Self {
                        self.$mname = $mname;
                        self
                    }
                )*
            }

            impl [<$name Cli>] {
                pub fn to_args(&self) -> [<$name Args>] {
                    [<$name Args>] {
                        $(
                            $fname: self.$fname.clone(),
                        )*

                        $(
                            $vname: self.$vname.clone(),
                        )*

                        $(
                            $mname: self.$mname.to_args(),
                        )*
                    }
                }
            }
        }
    };
}

pub use args;
pub use paste::paste;
