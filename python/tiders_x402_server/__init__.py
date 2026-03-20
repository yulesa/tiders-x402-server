from enum import Enum
from .tiders_x402_server import *

class Network(str, Enum):
    BASE_SEPOLIA = "base_sepolia"
    BASE = "base"
    XDC_MAINNET = "xdc_mainnet"
    AVALANCHE_FUJI = "avalanche_fuji"
    AVALANCHE = "avalanche"