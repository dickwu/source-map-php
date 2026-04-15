<?php

namespace HyperfTest\Cases;

use PHPUnit\Framework\Attributes\CoversClass;
use App\Controller\ConsentController;

class ConsentControllerTest
{
    #[CoversClass(ConsentController::class)]
    public function testStore(): void
    {
        $this->post('/consents');
    }
}
