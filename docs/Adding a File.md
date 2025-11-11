# Adding a File

Adding a file can by done simply by running
`config_share add_file <file_path>`. Which will default to telling which
file is which simply by the path of the file, however this may not
be your desired behaviour, so you can also pick a different method by
running `config_share add_file <file_path> --ask-method` which will prompt
you to select a method (either by a custom name or by the file path)

If the method that you select is by a custom name, then you will also have
to specify the full path to which the file is to be stored
