macro_rules! args {
    ($(#$argsmeta:tt)* $name:ident { $($fhelp:literal $fname:ident: $ftype:ty = $fdefault:expr;)* $($mname:ident: $mtype:ty;)* }) => {
        paste::paste! {
            #[derive(clap::Args, Debug)]
            pub struct [<$name Cli>] {
                $(
                    #[arg(long, default_value_t = ($fdefault), help = $fhelp)]
                    $fname: $ftype,
                )*

                $(
                    #[command(flatten)]
                    $mname: [<$mtype Cli>],
                )*
            }

            $(#$argsmeta)*
            pub struct [<$name Args>] {
                $(
                    $fname: $ftype,
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
                            $mname: self.$mname.to_args(),
                        )*
                    }
                }
            }
        }
    };
}

pub(super) use args;
