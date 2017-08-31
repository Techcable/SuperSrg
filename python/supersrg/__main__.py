from argh import ArghParser, arg

def convert():
    """Convert mappings from one format to another"""
    raise NotImplementedError("TODO")

def main():
    parser = ArghParser(description="Additional SuperSrg utilities")
    parser.add_commands([convert])
    parser.dispatch()
